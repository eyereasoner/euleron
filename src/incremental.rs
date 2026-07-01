//! Incremental (assert/retract) forward chaining over N3-authored rules,
//! applied to genuine RDF quads via `sophia_api` rather than a hand-rolled
//! fact representation.
//!
//! ## Why `sophia_api`
//!
//! `sophia_api::term::Term` has a first-class `Variable` kind ("a SPARQL or
//! Notation3 variable"), so the same term type serves as both the rule
//! *pattern* language (which needs variables) and the ground *data* it
//! matches against -- there is no separate pattern-term/ground-term type and
//! no conversion boundary between them.
//!
//! Facts are [`Quad`] (this module's own struct: `s`/`p`/`o`/`g: Option<_>`),
//! which implements `sophia_api::quad::Quad`, so it is a real,
//! standards-conforming quad usable with the rest of the sophia ecosystem
//! (parsers, stores, dataset traits) rather than an ad hoc struct nobody
//! else's code could consume.
//!
//! ## Rules vs. data
//!
//! Rules are still authored in N3 and parsed by eyeron's own parser into
//! [`crate::ast::Rule`] -- that doesn't change, since N3 rule syntax is the
//! whole point of this crate. [`IncrementalReasoner::new`] converts each
//! rule's premise/conclusion triples into this module's [`RdfTerm`] once, at
//! construction time. The same restrictions as before apply to rule *text*:
//! forward rules only; flat IRI/variable/literal terms only (no inline
//! blank-node or list *pattern* syntax); no built-in predicates other than
//! `rdf:first`/`rdf:rest` (see below).
//!
//! Facts (what you [`assert_fact`](IncrementalReasoner::assert_fact) /
//! [`retract_fact`](IncrementalReasoner::retract_fact)) are plain [`Quad`]
//! values -- there is no N3 parsing involved on the data path at all. Real
//! RDF has no "list" term kind (`rdf:List`s are always blank-node chains of
//! `rdf:first`/`rdf:rest`/`rdf:nil` triples), so unlike eyeron's own
//! N3-native AST (which has a first-class `Term::List` requiring special
//! evaluation), `rdf:first`/`rdf:rest` need no special-casing here at all --
//! they are just ordinary matchable predicates over ordinary quads that
//! happen, by convention, to encode a list.
//!
//! ## Graphs
//!
//! A rule firing only ever joins premises from the *same* graph (including
//! the default graph, `g: None`), and its derived quad is placed in that
//! same graph: reasoning behaves as if run independently per named graph,
//! sharing one engine/index. N3 has no syntax to name a graph in a rule, so
//! there is no way (yet) to write a rule that joins across graphs. This is
//! implemented directly: each distinct graph gets its own independent
//! [`GraphState`] (closure, index, justifications), and a quad is routed to
//! its graph's state before anything else happens.
//!
//! ## Design (within one graph)
//!
//! Every fact (explicit or derived) gets a stable `FactId` the first time it
//! is seen, and keeps it for the life of that graph's state (ids are never
//! reused, so stale references left over from a previous retraction are
//! always detectable). For every derived fact we record one [`Support`]
//! entry per rule firing that derives it, plus the ids of the ground
//! premises that firing used. `dependents` is the reverse index: for a
//! supporting fact, which derived facts currently cite it in some
//! justification.
//!
//! `assert_fact` is ordinary semi-naive evaluation: the new fact is joined
//! against the existing indexed facts for every premise position it could
//! fill, one rule at a time, and newly derived facts are queued so their own
//! consequences get computed too.
//!
//! `retract_fact` is the DRed (delete/rederive) algorithm: dropping a fact's
//! explicit support puts everything transitively reachable through
//! `dependents` under suspicion ("over-delete"), then a shrinking fixpoint
//! rescues any suspect that still has a justification whose premises are not
//! themselves under suspicion. Whatever is left condemned at the fixpoint is
//! truly gone.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use sophia_api::quad::{Quad as SophiaQuad, Spog};
use sophia_api::term::{BnodeId, IriRef, LanguageTag, SimpleTerm, VarName};
use sophia_api::MownStr;

use crate::ast::{self, LOG_IMPLIED_BY, LOG_IMPLIES};
use crate::error::{EyeronError, Result};
use crate::reasoner::is_builtin_iri;

/// The term type used throughout this module: an owned `sophia_api` term.
pub type RdfTerm = SimpleTerm<'static>;

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// Build an IRI term. Fails if `value` is not a valid absolute IRI.
pub fn iri(value: impl Into<String>) -> Result<RdfTerm> {
    IriRef::new(MownStr::from(value.into()))
        .map(RdfTerm::Iri)
        .map_err(|e| EyeronError::new(e.to_string()))
}

/// Build a blank node term. Fails if `label` is not a valid blank node label.
pub fn blank(label: impl Into<String>) -> Result<RdfTerm> {
    BnodeId::new(MownStr::from(label.into()))
        .map(RdfTerm::BlankNode)
        .map_err(|e| EyeronError::new(e.to_string()))
}

/// Build a pattern variable term. Fails if `name` is not a valid variable name.
pub fn var(name: impl Into<String>) -> Result<RdfTerm> {
    VarName::new(MownStr::from(name.into()))
        .map(RdfTerm::Variable)
        .map_err(|e| EyeronError::new(e.to_string()))
}

/// Build a plain (`xsd:string`) literal term.
pub fn literal(value: impl Into<String>) -> RdfTerm {
    let datatype = IriRef::new(MownStr::from(XSD_STRING)).expect("xsd:string is a valid IRI");
    RdfTerm::LiteralDatatype(MownStr::from(value.into()), datatype)
}

/// A real RDF quad: subject, predicate, object, and an optional graph name
/// (`None` = the default graph). Implements `sophia_api::quad::Quad`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Quad {
    pub s: RdfTerm,
    pub p: RdfTerm,
    pub o: RdfTerm,
    pub g: Option<RdfTerm>,
}

impl Quad {
    pub fn new(s: RdfTerm, p: RdfTerm, o: RdfTerm, g: Option<RdfTerm>) -> Self {
        Self { s, p, o, g }
    }
}

impl SophiaQuad for Quad {
    type Term = RdfTerm;
    type BorrowTerm<'x>
        = &'x RdfTerm
    where
        Self: 'x;

    fn s(&self) -> Self::BorrowTerm<'_> { &self.s }
    fn p(&self) -> Self::BorrowTerm<'_> { &self.p }
    fn o(&self) -> Self::BorrowTerm<'_> { &self.o }
    fn g(&self) -> Option<Self::BorrowTerm<'_>> { self.g.as_ref() }

    fn to_spog(self) -> Spog<Self::Term> {
        ([self.s, self.p, self.o], self.g)
    }
}

/// The set of quads that became newly true or newly false as the result of a
/// single `assert_fact` / `retract_fact` call.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Delta {
    pub added: Vec<Quad>,
    pub removed: Vec<Quad>,
}

impl Delta {
    fn merge(&mut self, other: Delta) {
        self.added.extend(other.added);
        self.removed.extend(other.removed);
    }
}

type FactId = usize;
type GraphKey = Option<RdfTerm>;
type Triple = (RdfTerm, RdfTerm, RdfTerm);
type Bindings = BTreeMap<String, RdfTerm>;

fn var_name(term: &RdfTerm) -> Option<&str> {
    match term {
        RdfTerm::Variable(name) => Some(name.as_str()),
        _ => None,
    }
}

fn ground_value(term: &RdfTerm, bindings: &Bindings) -> Option<RdfTerm> {
    match var_name(term) {
        Some(name) => bindings.get(name).cloned(),
        None => Some(term.clone()),
    }
}

fn resolve_flat(term: &RdfTerm, bindings: &Bindings) -> RdfTerm {
    match var_name(term) {
        Some(name) => bindings.get(name).cloned().unwrap_or_else(|| term.clone()),
        None => term.clone(),
    }
}

fn instantiate_ground(t: &Triple, bindings: &Bindings) -> Option<Triple> {
    let out = (
        resolve_flat(&t.0, bindings),
        resolve_flat(&t.1, bindings),
        resolve_flat(&t.2, bindings),
    );
    let ground = var_name(&out.0).is_none() && var_name(&out.1).is_none() && var_name(&out.2).is_none();
    ground.then_some(out)
}

fn match_term(pattern: &RdfTerm, value: &RdfTerm, bindings: &mut Bindings) -> bool {
    match var_name(pattern) {
        Some(name) => match bindings.get(name) {
            Some(existing) => existing == value,
            None => {
                bindings.insert(name.to_string(), value.clone());
                true
            }
        },
        None => pattern == value,
    }
}

fn match_triple(pattern: &Triple, fact: &Triple, bindings: &mut Bindings) -> bool {
    match_term(&pattern.0, &fact.0, bindings)
        && match_term(&pattern.1, &fact.1, bindings)
        && match_term(&pattern.2, &fact.2, bindings)
}

fn term_is_flat(t: &RdfTerm) -> bool {
    !matches!(t, RdfTerm::Triple(_))
}

/// `rdf:first`/`rdf:rest` are ordinary matchable predicates over real quads;
/// every other built-in is out of scope.
fn is_supported_list_accessor(iri: &str) -> bool {
    matches!(
        iri,
        ast::RDF_FIRST | ast::LIST_FIRST | ast::RDF_REST | ast::LIST_REST
    )
}

fn rule_triple_uses_builtin(t: &ast::Triple) -> bool {
    matches!(&t.p, ast::Term::Iri(p) if (is_builtin_iri(p) && !is_supported_list_accessor(p)) || p == LOG_IMPLIES || p == LOG_IMPLIED_BY)
}

fn convert_term(t: &ast::Term, rule_index: usize) -> Result<RdfTerm> {
    match t {
        ast::Term::Iri(value) => iri(value.clone())
            .map_err(|e| EyeronError::new(format!("rule {rule_index}: invalid IRI '{value}': {e}"))),
        ast::Term::Var(name) => var(name.clone())
            .map_err(|e| EyeronError::new(format!("rule {rule_index}: invalid variable name '{name}': {e}"))),
        ast::Term::Literal(lit) => convert_literal(lit, rule_index),
        ast::Term::Blank(_) => Err(EyeronError::new(format!(
            "incremental reasoner does not support inline blank-node pattern syntax in rules yet (rule {rule_index})"
        ))),
        ast::Term::List(_) | ast::Term::Formula(_) => Err(EyeronError::new(format!(
            "incremental reasoner does not support list/formula pattern syntax in rules yet (rule {rule_index})"
        ))),
    }
}

fn convert_literal(lit: &ast::Literal, rule_index: usize) -> Result<RdfTerm> {
    if let Some(lang) = &lit.language {
        let tag = LanguageTag::new(MownStr::from(lang.clone()))
            .map_err(|e| EyeronError::new(format!("rule {rule_index}: invalid language tag '{lang}': {e}")))?;
        return Ok(RdfTerm::LiteralLanguage(MownStr::from(lit.value.clone()), tag, None));
    }
    let datatype = lit.datatype.clone().unwrap_or_else(|| XSD_STRING.to_string());
    let datatype_iri = iri(datatype.clone())
        .map_err(|e| EyeronError::new(format!("rule {rule_index}: invalid datatype IRI '{datatype}': {e}")))?;
    let RdfTerm::Iri(datatype_iri) = datatype_iri else { unreachable!() };
    Ok(RdfTerm::LiteralDatatype(MownStr::from(lit.value.clone()), datatype_iri))
}

fn convert_triple(t: &ast::Triple, rule_index: usize) -> Result<Triple> {
    if rule_triple_uses_builtin(t) {
        return Err(EyeronError::new(format!(
            "incremental reasoner does not support built-in predicates yet (rule {rule_index}: {t})"
        )));
    }
    let s = convert_term(&t.s, rule_index)?;
    let p = convert_term(&t.p, rule_index)?;
    let o = convert_term(&t.o, rule_index)?;
    for term in [&s, &p, &o] {
        if !term_is_flat(term) {
            return Err(EyeronError::new(format!(
                "incremental reasoner does not support RDF-star quoted-triple terms yet (rule {rule_index}: {t})"
            )));
        }
    }
    Ok((s, p, o))
}

#[derive(Debug, Clone)]
struct Rule {
    premise: Vec<Triple>,
    conclusion: Vec<Triple>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Support {
    Explicit,
    Derived { rule_index: usize, premises: Vec<FactId> },
}

#[derive(Debug, Default)]
struct FactIndex {
    by_p: HashMap<RdfTerm, Vec<FactId>>,
    by_sp: HashMap<(RdfTerm, RdfTerm), Vec<FactId>>,
    by_po: HashMap<(RdfTerm, RdfTerm), Vec<FactId>>,
    all: Vec<FactId>,
}

impl FactIndex {
    fn insert(&mut self, id: FactId, t: &Triple) {
        self.by_p.entry(t.1.clone()).or_default().push(id);
        self.by_sp.entry((t.0.clone(), t.1.clone())).or_default().push(id);
        self.by_po.entry((t.1.clone(), t.2.clone())).or_default().push(id);
        self.all.push(id);
    }

    fn remove(&mut self, id: FactId, t: &Triple) {
        if let Some(v) = self.by_p.get_mut(&t.1) { v.retain(|x| *x != id); }
        if let Some(v) = self.by_sp.get_mut(&(t.0.clone(), t.1.clone())) { v.retain(|x| *x != id); }
        if let Some(v) = self.by_po.get_mut(&(t.1.clone(), t.2.clone())) { v.retain(|x| *x != id); }
        self.all.retain(|x| *x != id);
    }

    fn candidates(&self, pattern: &Triple, bindings: &Bindings) -> Vec<FactId> {
        let s = ground_value(&pattern.0, bindings);
        let p = ground_value(&pattern.1, bindings);
        let o = ground_value(&pattern.2, bindings);

        match (s, p, o) {
            (_, Some(p), Some(o)) => self.by_po.get(&(p, o)).cloned().unwrap_or_default(),
            (Some(s), Some(p), _) => self.by_sp.get(&(s, p)).cloned().unwrap_or_default(),
            (_, Some(p), _) => self.by_p.get(&p).cloned().unwrap_or_default(),
            _ => self.all.clone(),
        }
    }
}

#[derive(Debug, Default)]
struct GraphDelta {
    added: Vec<Triple>,
    removed: Vec<Triple>,
}

impl GraphDelta {
    fn into_delta(self, g: GraphKey) -> Delta {
        let added = self.added.into_iter().map(|(s, p, o)| Quad::new(s, p, o, g.clone())).collect();
        let removed = self.removed.into_iter().map(|(s, p, o)| Quad::new(s, p, o, g.clone())).collect();
        Delta { added, removed }
    }
}

#[derive(Debug, Default)]
struct GraphState {
    closure: Vec<Triple>,
    ids: HashMap<Triple, FactId>,
    index: FactIndex,
    justifications: HashMap<FactId, Vec<Support>>,
    dependents: HashMap<FactId, HashSet<FactId>>,
}

impl GraphState {
    fn contains(&self, fact: &Triple) -> bool {
        self.ids.contains_key(fact)
    }

    fn assert_fact(&mut self, rules: &[Rule], fact: Triple, delta: &mut GraphDelta) {
        if let Some(&id) = self.ids.get(&fact) {
            let entry = self.justifications.entry(id).or_default();
            if !entry.iter().any(|j| matches!(j, Support::Explicit)) {
                entry.push(Support::Explicit);
            }
            return;
        }

        let id = self.insert_fact(fact.clone());
        self.justifications.insert(id, vec![Support::Explicit]);
        delta.added.push(fact);

        let mut worklist = VecDeque::from([id]);
        while let Some(next) = worklist.pop_front() {
            self.propagate_from(rules, next, &mut worklist, delta);
        }
    }

    fn retract_fact(&mut self, fact: &Triple, delta: &mut GraphDelta) {
        let Some(&id) = self.ids.get(fact) else { return; };

        let Some(list) = self.justifications.get_mut(&id) else { return; };
        let had_explicit = list.iter().any(|j| matches!(j, Support::Explicit));
        if !had_explicit { return; }
        list.retain(|j| !matches!(j, Support::Explicit));
        if !list.is_empty() { return; }

        // Phase 1: over-delete. Every fact transitively reachable through
        // `dependents` from `id` is a suspect, not yet a confirmed removal.
        let mut condemned: HashSet<FactId> = HashSet::new();
        let mut stack = vec![id];
        while let Some(f) = stack.pop() {
            if condemned.insert(f) {
                if let Some(deps) = self.dependents.get(&f) {
                    stack.extend(deps.iter().copied());
                }
            }
        }

        // Phase 2: rederive. Shrink `condemned` until no suspect has a
        // surviving justification whose premises are all outside it.
        loop {
            let mut rescued = Vec::new();
            for &f in &condemned {
                if f == id { continue; }
                let supported = self.justifications.get(&f).into_iter().flatten().any(|j| match j {
                    Support::Explicit => true,
                    Support::Derived { premises, .. } => premises.iter().all(|p| !condemned.contains(p)),
                });
                if supported { rescued.push(f); }
            }
            if rescued.is_empty() { break; }
            for f in rescued { condemned.remove(&f); }
        }

        // Phase 3: whatever remains condemned is truly gone.
        for f in &condemned {
            delta.removed.push(self.closure[*f].clone());
        }
        for f in condemned {
            self.finalize_removal(f);
        }
    }

    fn insert_fact(&mut self, fact: Triple) -> FactId {
        let id = self.closure.len();
        self.index.insert(id, &fact);
        self.ids.insert(fact.clone(), id);
        self.closure.push(fact);
        id
    }

    fn finalize_removal(&mut self, id: FactId) {
        let fact = self.closure[id].clone();
        self.index.remove(id, &fact);
        self.ids.remove(&fact);

        if let Some(justs) = self.justifications.remove(&id) {
            for j in justs {
                if let Support::Derived { premises, .. } = j {
                    for p in premises {
                        if let Some(deps) = self.dependents.get_mut(&p) { deps.remove(&id); }
                    }
                }
            }
        }

        if let Some(dependents) = self.dependents.remove(&id) {
            for dep in dependents {
                if let Some(list) = self.justifications.get_mut(&dep) {
                    list.retain(|j| match j {
                        Support::Explicit => true,
                        Support::Derived { premises, .. } => !premises.contains(&id),
                    });
                }
            }
        }
    }

    fn propagate_from(&mut self, rules: &[Rule], id: FactId, worklist: &mut VecDeque<FactId>, delta: &mut GraphDelta) {
        let fact = self.closure[id].clone();
        for (rule_index, rule) in rules.iter().enumerate() {
            for (prem_idx, premise) in rule.premise.iter().enumerate() {
                let mut bindings = Bindings::new();
                if !match_triple(premise, &fact, &mut bindings) { continue; }

                let remaining: Vec<usize> = (0..rule.premise.len()).filter(|&j| j != prem_idx).collect();
                let mut solutions = Vec::new();
                self.join(&rule.premise, &remaining, bindings, vec![id], &mut solutions);

                for (sol_bindings, used_ids) in solutions {
                    for head in &rule.conclusion {
                        if let Some(concl) = instantiate_ground(head, &sol_bindings) {
                            self.add_derived(concl, rule_index, used_ids.clone(), worklist, delta);
                        }
                    }
                }
            }
        }
    }

    fn join(
        &self,
        premises: &[Triple],
        remaining: &[usize],
        bindings: Bindings,
        used: Vec<FactId>,
        out: &mut Vec<(Bindings, Vec<FactId>)>,
    ) {
        let Some((&i, rest)) = remaining.split_first() else {
            out.push((bindings, used));
            return;
        };
        for cand_id in self.index.candidates(&premises[i], &bindings) {
            let mut b = bindings.clone();
            if match_triple(&premises[i], &self.closure[cand_id], &mut b) {
                let mut u = used.clone();
                u.push(cand_id);
                self.join(premises, rest, b, u, out);
            }
        }
    }

    fn add_derived(
        &mut self,
        fact: Triple,
        rule_index: usize,
        mut used_ids: Vec<FactId>,
        worklist: &mut VecDeque<FactId>,
        delta: &mut GraphDelta,
    ) {
        used_ids.sort_unstable();
        used_ids.dedup();
        let support = Support::Derived { rule_index, premises: used_ids.clone() };

        let id = if let Some(&existing) = self.ids.get(&fact) {
            existing
        } else {
            let id = self.insert_fact(fact.clone());
            delta.added.push(fact);
            worklist.push_back(id);
            id
        };

        let entry = self.justifications.entry(id).or_default();
        if !entry.contains(&support) {
            entry.push(support);
            for p in &used_ids {
                self.dependents.entry(*p).or_default().insert(id);
            }
        }
    }
}

/// Incremental forward-chaining reasoner. See the module docs for the
/// supported rule subset and how graphs are handled.
pub struct IncrementalReasoner {
    rules: Vec<Rule>,
    graphs: HashMap<GraphKey, GraphState>,
}

impl IncrementalReasoner {
    pub fn new(rules: Vec<ast::Rule>) -> Result<Self> {
        let mut converted = Vec::with_capacity(rules.len());
        for (i, rule) in rules.iter().enumerate() {
            if !rule.is_forward {
                return Err(EyeronError::new(format!(
                    "incremental reasoner does not support backward rules yet (rule {i})"
                )));
            }
            let premise = rule.premise.iter().map(|t| convert_triple(t, i)).collect::<Result<Vec<_>>>()?;
            let conclusion = rule.conclusion.iter().map(|t| convert_triple(t, i)).collect::<Result<Vec<_>>>()?;
            converted.push(Rule { premise, conclusion });
        }
        Ok(Self { rules: converted, graphs: HashMap::new() })
    }

    /// Currently-true quads (explicit or derived), across all graphs.
    pub fn closure(&self) -> Vec<Quad> {
        self.graphs
            .iter()
            .flat_map(|(g, state)| {
                state.ids.keys().map(move |(s, p, o)| Quad::new(s.clone(), p.clone(), o.clone(), g.clone()))
            })
            .collect()
    }

    pub fn contains(&self, quad: &Quad) -> bool {
        self.graphs
            .get(&quad.g)
            .is_some_and(|state| state.contains(&(quad.s.clone(), quad.p.clone(), quad.o.clone())))
    }

    /// Assert one quad as explicitly true. Returns everything that became
    /// newly true as a result (including `quad` itself, unless it was
    /// already known). Rule firing only ever joins premises from `quad.g`.
    pub fn assert_fact(&mut self, quad: Quad) -> Delta {
        let Quad { s, p, o, g } = quad;
        let mut delta = GraphDelta::default();
        let Self { rules, graphs } = self;
        let state = graphs.entry(g.clone()).or_default();
        state.assert_fact(rules, (s, p, o), &mut delta);
        delta.into_delta(g)
    }

    /// Convenience for seeding many quads at once; equivalent to calling
    /// `assert_fact` for each one and merging the deltas.
    pub fn assert_all(&mut self, facts: impl IntoIterator<Item = Quad>) -> Delta {
        let mut delta = Delta::default();
        for fact in facts {
            delta.merge(self.assert_fact(fact));
        }
        delta
    }

    /// Retract a quad's *explicit* support. If it is still derivable some
    /// other way, the visible closure does not change. Otherwise this cascades
    /// (DRed: over-delete then rederive) to everything that was only true
    /// because of this quad, within the same graph.
    pub fn retract_fact(&mut self, quad: &Quad) -> Delta {
        let mut delta = GraphDelta::default();
        if let Some(state) = self.graphs.get_mut(&quad.g) {
            state.retract_fact(&(quad.s.clone(), quad.p.clone(), quad.o.clone()), &mut delta);
        }
        delta.into_delta(quad.g.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub_property_transitivity() -> ast::Rule {
        // { ?a <subPropertyOf> ?b . ?b <subPropertyOf> ?c } => { ?a <subPropertyOf> ?c } .
        ast::Rule {
            premise: vec![
                ast::Triple::new(ast::Term::var("a"), ast::Term::iri("subPropertyOf"), ast::Term::var("b")),
                ast::Triple::new(ast::Term::var("b"), ast::Term::iri("subPropertyOf"), ast::Term::var("c")),
            ],
            conclusion: vec![ast::Triple::new(ast::Term::var("a"), ast::Term::iri("subPropertyOf"), ast::Term::var("c"))],
            is_forward: true,
        }
    }

    fn q(s: &str, p: &str, o: &str) -> Quad {
        Quad::new(iri(s).unwrap(), iri(p).unwrap(), iri(o).unwrap(), None)
    }

    #[test]
    fn asserting_bridges_transitive_chain() {
        let mut r = IncrementalReasoner::new(vec![sub_property_transitivity()]).unwrap();
        r.assert_fact(q("p", "subPropertyOf", "q"));
        let delta = r.assert_fact(q("q", "subPropertyOf", "r"));
        assert!(delta.added.contains(&q("p", "subPropertyOf", "r")));
    }

    #[test]
    fn redundant_support_survives_one_retraction() {
        let mut r = IncrementalReasoner::new(vec![sub_property_transitivity()]).unwrap();
        let pq = q("p", "subPropertyOf", "q");
        let qr = q("q", "subPropertyOf", "r");
        let pr_direct = q("p", "subPropertyOf", "r");
        r.assert_fact(pq);
        r.assert_fact(qr.clone());
        // Also assert the conclusion directly, so it now has two supports:
        // one explicit, one derived via p-q-r.
        r.assert_fact(pr_direct.clone());

        let delta = r.retract_fact(&qr);
        assert!(!delta.removed.contains(&pr_direct), "explicit support should keep it alive");
        assert!(r.contains(&pr_direct));

        // Now remove the explicit assertion too -- no support left, it must go.
        let delta2 = r.retract_fact(&pr_direct);
        assert!(delta2.removed.contains(&pr_direct));
        assert!(!r.contains(&pr_direct));
    }
}
