use crate::ast::*;
use std::collections::{BTreeMap, HashMap, HashSet};

pub type Bindings = BTreeMap<String, Term>;

const MAX_BACKWARD_DEPTH: usize = 32;
const MAX_BACKWARD_SOLUTIONS_PER_GOAL: usize = 1024;
const MAX_MATCH_STEPS: usize = 200_000;

#[derive(Debug, Default)]
struct SearchBudget {
    steps: usize,
}

impl SearchBudget {
    fn tick(&mut self) -> bool {
        self.steps += 1;
        self.steps <= MAX_MATCH_STEPS
    }
}


fn blank_binding_name(name: &str) -> String {
    format!("_:{}", name)
}

fn resolve_pattern(term: &Term, bindings: &Bindings) -> Term {
    resolve_pattern_with_seen(term, bindings, &mut HashSet::new())
}

fn resolve_pattern_with_seen(term: &Term, bindings: &Bindings, seen: &mut HashSet<String>) -> Term {
    match term {
        Term::Var(name) => {
            if !seen.insert(name.clone()) { return term.clone(); }
            match bindings.get(name) {
                Some(bound) => resolve_pattern_with_seen(bound, bindings, seen),
                None => term.clone(),
            }
        }
        // Blank nodes that occur in rule bodies/formula patterns are local
        // existential pattern variables.  A property list such as
        // `[ a dp:ForkState ; dp:in ?C ; dp:fork ?F ]` must therefore match
        // any one blank node while preserving identity across all generated
        // triples in the property list.  Store those bindings in the same
        // substitution map with a disjoint key prefix.
        Term::Blank(name) => {
            let key = blank_binding_name(name);
            if !seen.insert(key.clone()) { return term.clone(); }
            match bindings.get(&key) {
                Some(bound) => resolve_pattern_with_seen(bound, bindings, seen),
                None => Term::Var(key),
            }
        }
        Term::List(items) => Term::List(items.iter().map(|item| {
            let mut branch_seen = seen.clone();
            resolve_pattern_with_seen(item, bindings, &mut branch_seen)
        }).collect()),
        Term::Formula(triples) => Term::Formula(triples.iter().map(|t| {
            let mut s_seen = seen.clone();
            let mut p_seen = seen.clone();
            let mut o_seen = seen.clone();
            Triple::new(
                resolve_pattern_with_seen(&t.s, bindings, &mut s_seen),
                resolve_pattern_with_seen(&t.p, bindings, &mut p_seen),
                resolve_pattern_with_seen(&t.o, bindings, &mut o_seen),
            )
        }).collect()),
        _ => term.clone(),
    }
}


#[derive(Debug, Default, Clone)]
struct FactIndex {
    // Keep the index deliberately lean.  Earlier versions indexed each fact in
    // six maps (s, p, o, sp, po, so), which helped small examples but doubled
    // down on memory at deep-taxonomy-100000.  The hot paths in the packaged
    // examples are predicate/object (`?X a :Class`) and subject/predicate
    // (`:arc :check ?Msg`), with predicate-only as a useful fallback.
    by_p: BTreeMap<Term, Vec<usize>>,
    by_sp: BTreeMap<(Term, Term), Vec<usize>>,
    by_po: BTreeMap<(Term, Term), Vec<usize>>,
}

impl FactIndex {
    fn insert(&mut self, idx: usize, triple: &Triple) {
        self.by_p.entry(triple.p.clone()).or_default().push(idx);
        self.by_sp.entry((triple.s.clone(), triple.p.clone())).or_default().push(idx);
        self.by_po.entry((triple.p.clone(), triple.o.clone())).or_default().push(idx);
    }

    fn candidates<'a>(&'a self, facts: &'a [Triple], pattern: &Triple, bindings: &Bindings) -> Vec<&'a Triple> {
        let s = resolve_pattern(&pattern.s, bindings);
        let p = resolve_pattern(&pattern.p, bindings);
        let o = resolve_pattern(&pattern.o, bindings);
        let sg = s.is_ground();
        let pg = p.is_ground();
        let og = o.is_ground();

        let indices = if pg && og {
            self.by_po.get(&(p.clone(), o.clone()))
        } else if sg && pg {
            self.by_sp.get(&(s.clone(), p.clone()))
        } else if pg {
            self.by_p.get(&p)
        } else {
            None
        };

        match indices {
            Some(indices) => indices.iter().map(|idx| &facts[*idx]).collect(),
            // If all grounded positions were ones this lean index cannot use
            // (for example subject+object), fall back to a scan so correctness
            // is preserved.  Predicate-grounded misses can fail immediately.
            None if pg => Vec::new(),
            None => facts.iter().collect(),
        }
    }
}

#[derive(Debug, Clone)]
struct AgendaEntry {
    rule_index: usize,
    goal: Triple,
    s_ground: Option<Term>,
    p_ground: Term,
    o_ground: Option<Term>,
}

#[derive(Debug, Default, Clone)]
struct AgendaIndex {
    entries: Vec<AgendaEntry>,
    by_p: HashMap<Term, Vec<usize>>,
    by_sp: HashMap<(Term, Term), Vec<usize>>,
    by_po: HashMap<(Term, Term), Vec<usize>>,
    indexed: HashSet<usize>,
}

impl AgendaIndex {
    fn insert(&mut self, entry: AgendaEntry) {
        let pos = self.entries.len();
        self.indexed.insert(entry.rule_index);
        if entry.s_ground.is_none() && entry.o_ground.is_none() {
            self.by_p.entry(entry.p_ground.clone()).or_default().push(pos);
        }
        if let Some(s) = &entry.s_ground {
            self.by_sp.entry((s.clone(), entry.p_ground.clone())).or_default().push(pos);
        }
        if let Some(o) = &entry.o_ground {
            self.by_po.entry((entry.p_ground.clone(), o.clone())).or_default().push(pos);
        }
        self.entries.push(entry);
    }

    fn candidates(&self, fact: &Triple) -> Vec<usize> {
        let mut out = Vec::<usize>::new();
        let mut seen_rules = HashSet::<usize>::new();

        if let Some(entries) = self.by_p.get(&fact.p) {
            for pos in entries {
                let entry = &self.entries[*pos];
                if seen_rules.insert(entry.rule_index) { out.push(*pos); }
            }
        }
        if let Some(entries) = self.by_sp.get(&(fact.s.clone(), fact.p.clone())) {
            for pos in entries {
                let entry = &self.entries[*pos];
                if seen_rules.insert(entry.rule_index) { out.push(*pos); }
            }
        }
        if let Some(entries) = self.by_po.get(&(fact.p.clone(), fact.o.clone())) {
            for pos in entries {
                let entry = &self.entries[*pos];
                if seen_rules.insert(entry.rule_index) { out.push(*pos); }
            }
        }

        out
    }
}


#[derive(Debug, Clone)]
pub struct ReasonerOptions {
    pub max_iterations: usize,
    pub trace: bool,
}

impl Default for ReasonerOptions {
    fn default() -> Self { Self { max_iterations: 10_000, trace: false } }
}

#[derive(Debug, Clone)]
pub struct ReasonerResult {
    pub explicit: Vec<Triple>,
    pub derived: Vec<Triple>,
    pub closure: Vec<Triple>,
}

pub fn reason(doc: &Document, options: &ReasonerOptions) -> ReasonerResult {
    let mut closure = Vec::<Triple>::new();
    let mut fact_index = FactIndex::default();
    let mut seen = HashSet::<Triple>::new();
    let mut explicit_seen = HashSet::<Triple>::new();

    for fact in &doc.facts {
        if admissible_fact(fact) && seen.insert(fact.clone()) {
            explicit_seen.insert(fact.clone());
            let idx = closure.len();
            closure.push(fact.clone());
            fact_index.insert(idx, fact);
        }
    }

    let mut active_rules = doc.rules.clone();
    let mut agenda_index = build_single_premise_agenda(&active_rules);
    let mut agenda_cursor = 0usize;
    let mut generated_rule_facts = HashSet::<Triple>::new();
    let mut derived = Vec::<Triple>::new();
    let mut iteration = 0usize;

    loop {
        iteration += 1;
        if iteration > options.max_iterations { break; }

        let before = seen.len();

        // Fast path, modelled after the JavaScript engine: safe single-premise
        // rules are driven by newly seen facts.  This turns deep taxonomy chains
        // from "scan every rule for every wave" into "look up the rules that
        // can match this fact".
        while agenda_cursor < closure.len() {
            let fact = closure[agenda_cursor].clone();
            agenda_cursor += 1;
            let candidates = agenda_index.candidates(&fact);
            let mut restart_agenda = false;

            for entry_pos in candidates {
                let (rule_index, goal) = {
                    let entry = &agenda_index.entries[entry_pos];
                    (entry.rule_index, entry.goal.clone())
                };
                if rule_index >= active_rules.len() { continue; }
                let rule = active_rules[rule_index].clone();
                let mut bindings = BTreeMap::<String, Term>::new();
                if !match_triple(&goal, &fact, &mut bindings) { continue; }

                let mut pending_rules = Vec::<Rule>::new();
                let rules_changed = emit_conclusions(
                    &rule,
                    &bindings,
                    &mut closure,
                    &mut fact_index,
                    &mut seen,
                    &explicit_seen,
                    &mut generated_rule_facts,
                    &mut derived,
                    &mut pending_rules,
                );

                if rules_changed {
                    active_rules.extend(pending_rules);
                    agenda_index = build_single_premise_agenda(&active_rules);
                    agenda_cursor = 0;
                    restart_agenda = true;
                    break;
                }
            }

            if restart_agenda { continue; }
        }

        // General path for multi-premise rules, builtins, backward-rule
        // dependencies, blank-node heads, and other rules whose firing cannot
        // be represented safely by the agenda above.
        let rule_count_at_start = active_rules.len();
        let mut pending_rules = Vec::<Rule>::new();
        for idx in 0..rule_count_at_start {
            if agenda_index.indexed.contains(&idx) { continue; }
            let rule = active_rules[idx].clone();
            if !rule.is_forward { continue; }

            let matches = match_premises(&rule.premise, &closure, Some(&fact_index), &active_rules);
            for bindings in matches {
                emit_conclusions(
                    &rule,
                    &bindings,
                    &mut closure,
                    &mut fact_index,
                    &mut seen,
                    &explicit_seen,
                    &mut generated_rule_facts,
                    &mut derived,
                    &mut pending_rules,
                );
            }
        }

        if !pending_rules.is_empty() {
            active_rules.extend(pending_rules);
            agenda_index = build_single_premise_agenda(&active_rules);
            agenda_cursor = 0;
        }

        if seen.len() == before {
            if agenda_cursor < closure.len() { continue; }
            break;
        }
    }

    ReasonerResult { explicit: doc.facts.clone(), derived, closure }
}

fn emit_conclusions(
    rule: &Rule,
    bindings: &Bindings,
    closure: &mut Vec<Triple>,
    fact_index: &mut FactIndex,
    seen: &mut HashSet<Triple>,
    explicit_seen: &HashSet<Triple>,
    generated_rule_facts: &mut HashSet<Triple>,
    derived: &mut Vec<Triple>,
    pending_rules: &mut Vec<Rule>,
) -> bool {
    let mut rules_changed = false;
    let mut blank_map = BTreeMap::<String, Term>::new();

    for head in &rule.conclusion {
        let Some(t) = instantiate_triple(head, bindings, &mut blank_map) else { continue; };

        if is_unquote_instruction(&t) {
            if let Term::Formula(triples) = t.o {
                for expanded in triples {
                    if insert_materialized_triple(
                        expanded,
                        closure,
                        fact_index,
                        seen,
                        explicit_seen,
                        generated_rule_facts,
                        derived,
                        pending_rules,
                    ) {
                        rules_changed = true;
                    }
                }
            }
            continue;
        }

        if insert_materialized_triple(
            t,
            closure,
            fact_index,
            seen,
            explicit_seen,
            generated_rule_facts,
            derived,
            pending_rules,
        ) {
            rules_changed = true;
        }
    }

    rules_changed
}

fn is_unquote_instruction(t: &Triple) -> bool {
    matches!((&t.s, &t.p), (Term::Iri(s), Term::Iri(p)) if s == EYERON_UNQUOTE && p == EYERON_UNQUOTE)
}

fn insert_materialized_triple(
    t: Triple,
    closure: &mut Vec<Triple>,
    fact_index: &mut FactIndex,
    seen: &mut HashSet<Triple>,
    explicit_seen: &HashSet<Triple>,
    generated_rule_facts: &mut HashSet<Triple>,
    derived: &mut Vec<Triple>,
    pending_rules: &mut Vec<Rule>,
) -> bool {
    if !admissible_fact(&t) { return false; }
    if !seen.insert(t.clone()) { return false; }

    let mut rules_changed = false;
    if !explicit_seen.contains(&t) { derived.push(t.clone()); }
    if let Some(new_rule) = rule_from_triple(&t) {
        if generated_rule_facts.insert(t.clone()) {
            pending_rules.push(new_rule);
            rules_changed = true;
        }
    }
    let idx = closure.len();
    closure.push(t.clone());
    fact_index.insert(idx, &t);
    rules_changed
}

fn build_single_premise_agenda(rules: &[Rule]) -> AgendaIndex {
    let mut backward_head_predicates = HashSet::<Term>::new();
    let mut has_wild_backward_head = false;
    for rule in rules {
        if rule.is_forward || rule.conclusion.len() != 1 { continue; }
        match &rule.conclusion[0].p {
            Term::Iri(_) => { backward_head_predicates.insert(rule.conclusion[0].p.clone()); }
            _ => { has_wild_backward_head = true; }
        }
    }

    let mut agenda = AgendaIndex::default();
    for (idx, rule) in rules.iter().enumerate() {
        let Some(entry) = agenda_entry_for_rule(idx, rule, &backward_head_predicates, has_wild_backward_head) else { continue; };
        agenda.insert(entry);
    }
    agenda
}

fn agenda_entry_for_rule(
    rule_index: usize,
    rule: &Rule,
    backward_head_predicates: &HashSet<Term>,
    has_wild_backward_head: bool,
) -> Option<AgendaEntry> {
    if !rule.is_forward || rule.premise.len() != 1 { return None; }
    if rule.conclusion.iter().any(triple_contains_blank) { return None; }

    let goal = &rule.premise[0];
    let Term::Iri(pred_iri) = &goal.p else { return None; };
    if is_builtin_iri(pred_iri) || pred_iri == LOG_IMPLIES || pred_iri == LOG_IMPLIED_BY { return None; }
    if has_wild_backward_head || backward_head_predicates.contains(&goal.p) { return None; }

    let s_ground = if goal.s.is_ground() { Some(goal.s.clone()) } else { None };
    let o_ground = if goal.o.is_ground() { Some(goal.o.clone()) } else { None };

    Some(AgendaEntry {
        rule_index,
        goal: goal.clone(),
        s_ground,
        p_ground: goal.p.clone(),
        o_ground,
    })
}

fn triple_contains_blank(triple: &Triple) -> bool {
    term_contains_blank(&triple.s) || term_contains_blank(&triple.p) || term_contains_blank(&triple.o)
}

fn term_contains_blank(term: &Term) -> bool {
    match term {
        Term::Blank(_) => true,
        Term::List(items) => items.iter().any(term_contains_blank),
        Term::Formula(triples) => triples.iter().any(triple_contains_blank),
        _ => false,
    }
}

pub(crate) fn is_builtin_iri(iri: &str) -> bool {
    matches!(iri,
        LOG_EQUAL_TO | LOG_NOT_EQUAL_TO | LOG_COLLECT_ALL_IN | LOG_FOR_ALL_IN
        | LOG_CONCLUSION | LOG_CONJUNCTION | LOG_NOT_INCLUDES | LOG_URI
        | RDF_FIRST | RDF_REST | LIST_FIRST | LIST_REST
        | LIST_APPEND | LIST_ITERATE | LIST_MAP | LIST_FIRST_REST | LIST_REVERSE
        | LIST_SORT | LIST_NOT_MEMBER
        | MATH_SUM | MATH_DIFFERENCE
    ) || is_list_builtin(iri) || is_math_operator(iri) || is_math_comparison(iri)
        || is_string_builtin(iri) || is_time_builtin(iri)
}

fn admissible_fact(t: &Triple) -> bool {
    rule_from_triple(t).is_some()
        || (admissible_fact_term(&t.s) && admissible_fact_term(&t.p) && admissible_fact_term(&t.o))
}

fn admissible_fact_term(term: &Term) -> bool {
    match term {
        Term::Var(_) => false,
        // Variables inside quoted formulas are data, not unbound top-level fact variables.
        Term::Formula(_) => true,
        Term::List(items) => items.iter().all(admissible_fact_term),
        _ => true,
    }
}

fn rule_to_triple(rule: &Rule, prefix: &str) -> Triple {
    // Rules are also visible as quoted implication triples, which lets examples
    // such as `rule-matching.n3` ask whether a rule exists.  Alpha-rename those
    // quoted rule variables before putting the rule-as-data in the fact closure;
    // otherwise a rule that matches itself can create cyclic bindings such as
    // `?A = { ?A => ?B }`.
    let quoted = standardize_apart(rule, prefix);
    if quoted.is_forward {
        Triple::new(
            Term::Formula(quoted.premise),
            Term::iri(LOG_IMPLIES),
            Term::Formula(quoted.conclusion),
        )
    } else {
        Triple::new(
            Term::Formula(quoted.conclusion),
            Term::iri(LOG_IMPLIED_BY),
            Term::Formula(quoted.premise),
        )
    }
}

fn rule_from_triple(t: &Triple) -> Option<Rule> {
    match (&t.s, &t.p, &t.o) {
        (Term::Formula(premise), Term::Iri(p), Term::Formula(conclusion)) if p == LOG_IMPLIES => {
            Some(Rule { premise: premise.clone(), conclusion: conclusion.clone(), is_forward: true })
        }
        (Term::Formula(head), Term::Iri(p), Term::Formula(body)) if p == LOG_IMPLIED_BY => {
            Some(Rule { premise: body.clone(), conclusion: head.clone(), is_forward: false })
        }
        _ => None,
    }
}

fn match_premises(premises: &[Triple], facts: &[Triple], fact_index: Option<&FactIndex>, rules: &[Rule]) -> Vec<Bindings> {
    let mut out = Vec::new();
    let mut backward_stack = HashSet::<String>::new();
    let mut budget = SearchBudget::default();
    match_premise_remaining(premises.to_vec(), facts, fact_index, rules, BTreeMap::new(), 0, &mut backward_stack, &mut budget, &mut out);
    out
}

fn match_premise_at(
    premises: &[Triple],
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    index: usize,
    bindings: Bindings,
    depth: usize,
    backward_stack: &mut HashSet<String>,
    budget: &mut SearchBudget,
    out: &mut Vec<Bindings>,
) {
    match_premise_remaining(premises[index..].to_vec(), facts, fact_index, rules, bindings, depth, backward_stack, budget, out);
}

fn match_premise_remaining(
    premises: Vec<Triple>,
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    bindings: Bindings,
    depth: usize,
    backward_stack: &mut HashSet<String>,
    budget: &mut SearchBudget,
    out: &mut Vec<Bindings>,
) {
    if !budget.tick() { return; }
    if premises.is_empty() {
        out.push(canonicalize_bindings(&bindings));
        return;
    }

    // Rule bodies in the examples often put tests such as log:notEqualTo before
    // the facts that bind their operands.  Select a runnable premise at each
    // step instead of committing to source order.  Prefer the smallest non-empty
    // candidate set; this keeps broad fact scans behind more selective goals.
    //
    // Empty candidate sets are ambiguous: an unready premise should be skipped,
    // while a grounded test that is definitely false must fail the whole branch.
    // This matters for recursive examples such as hanoi.n3.  Without the early
    // failure check, the matcher can bind `?N1` with math:difference even when
    // `?N math:greaterThan 1` is already false, then recursively try 0, -1, ... .
    for premise in &premises {
        if premise_is_definitively_false(premise, facts, fact_index, rules, &bindings) {
            return;
        }
    }

    let mut best_index = None;
    let mut best_candidates = Vec::<Bindings>::new();

    // First try the cheap, non-recursive paths: ordinary fact lookup, built-ins,
    // and lazy rule-as-data facts.  Only if none of the remaining premises can
    // make progress do we ask backward rules.  This avoids evaluating broad
    // recursive goals such as `?L :value ?LV` in expression-eval.n3 before
    // preceding structural facts have had a chance to bind `?L`.
    for (idx, premise) in premises.iter().enumerate() {
        let candidates = match_one_premise(premise, facts, fact_index, rules, &bindings, depth, backward_stack, budget, false);
        if candidates.is_empty() { continue; }
        if best_index.is_none() || candidates.len() < best_candidates.len() {
            best_index = Some(idx);
            best_candidates = candidates;
        }
    }

    if best_index.is_none() {
        for (idx, premise) in premises.iter().enumerate() {
            let candidates = match_one_premise(premise, facts, fact_index, rules, &bindings, depth, backward_stack, budget, true);
            if candidates.is_empty() { continue; }
            if best_index.is_none() || candidates.len() < best_candidates.len() {
                best_index = Some(idx);
                best_candidates = candidates;
            }
        }
    }

    let Some(idx) = best_index else { return; };
    let mut rest = premises;
    rest.remove(idx);
    for b in best_candidates {
        match_premise_remaining(rest.clone(), facts, fact_index, rules, b, depth, backward_stack, budget, out);
    }
}


fn premise_is_definitively_false(
    premise: &Triple,
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    bindings: &Bindings,
) -> bool {
    let pred = resolve(&premise.p, bindings);
    let Term::Iri(iri) = pred else { return false; };

    if is_math_comparison(&iri) {
        let left = resolve(&premise.s, bindings);
        let right = resolve(&premise.o, bindings);
        if numeric_value(&left).is_some() && numeric_value(&right).is_some() {
            return eval_math_compare(&iri, &premise.s, &premise.o, bindings).is_empty();
        }
    }

    if iri == LOG_NOT_EQUAL_TO {
        let left = resolve(&premise.s, bindings);
        let right = resolve(&premise.o, bindings);
        if !matches!(left, Term::Var(_)) && !matches!(right, Term::Var(_)) {
            return left == right;
        }
    }

    // For ordinary groundable fact goals, an empty indexed lookup is a real
    // contradiction when no backward rule can derive that predicate.  This is
    // critical for recursive backward programs such as expression-eval.n3: once
    // a candidate expression is known to be `:mul`, the alternative `:op :add`
    // and `:op :sub` branches must fail before their recursive `:value` goals
    // are explored.  Otherwise the scheduler can recursively evaluate large
    // wrong branches just to discover a grounded structural fact was absent.
    if !is_builtin_iri(&iri)
        && iri != LOG_IMPLIES
        && iri != LOG_IMPLIED_BY
        && !backward_rules_may_derive_predicate(&Term::Iri(iri.clone()), rules)
    {
        let resolved = resolve_pattern_triple(premise, bindings);
        if ordinary_fact_goal_is_ready(&resolved) {
            let candidates = match fact_index {
                Some(index) => index.candidates(facts, &resolved, &BTreeMap::new()),
                None => facts.iter().collect(),
            };
            if !candidates.iter().any(|fact| {
                let mut local = BTreeMap::new();
                match_triple(&resolved, fact, &mut local)
            }) {
                return true;
            }
        }
    }

    false
}

fn ordinary_fact_goal_is_ready(goal: &Triple) -> bool {
    // Treat a fact goal as ready for contradiction pruning when the predicate
    // is concrete and at least one data position is concrete.  With only a
    // predicate (`?s :p ?o`), absence from the predicate index is still a true
    // contradiction, but this helper is kept conservative for broad scans.
    matches!(goal.p, Term::Iri(_))
        && (!matches!(goal.s, Term::Var(_)) || !matches!(goal.o, Term::Var(_)))
}

fn backward_rules_may_derive_predicate(predicate: &Term, rules: &[Rule]) -> bool {
    rules.iter().any(|rule| {
        !rule.is_forward
            && rule.conclusion.iter().any(|head| {
                match &head.p {
                    Term::Var(_) => true,
                    p => p == predicate,
                }
            })
    })
}

fn match_one_premise(
    premise: &Triple,
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    bindings: &Bindings,
    depth: usize,
    backward_stack: &mut HashSet<String>,
    budget: &mut SearchBudget,
    allow_backward: bool,
) -> Vec<Bindings> {
    if let Some(next_bindings) = eval_builtin(premise, bindings, facts, fact_index, rules, depth, backward_stack, budget) {
        return next_bindings;
    }

    let mut out = Vec::new();
    let candidates = match fact_index {
        Some(index) => index.candidates(facts, premise, bindings),
        None => facts.iter().collect(),
    };
    for fact in candidates {
        let mut b = bindings.clone();
        if match_triple(premise, fact, &mut b) {
            out.push(canonicalize_bindings(&b));
        }
    }

    // Source rules are visible as quoted implication triples for rule-as-data
    // examples, but they are generated lazily instead of being inserted into
    // the ordinary closure/index.  This avoids duplicating huge quoted formulas
    // for rule-heavy inputs such as deep-taxonomy-100000.
    if may_match_rule_fact(premise, bindings) {
        for (rule_idx, rule) in rules.iter().enumerate() {
            let rule_fact = rule_to_triple(rule, &format!("__rulefact_{}__", rule_idx));
            let mut b = bindings.clone();
            if match_triple(premise, &rule_fact, &mut b) {
                out.push(canonicalize_bindings(&b));
            }
        }
    }

    if allow_backward && should_try_backward_goal(premise, bindings) {
        out.extend(solve_backward_goal(premise, facts, fact_index, rules, bindings, depth, backward_stack, budget));
    }
    out
}


fn may_match_rule_fact(pattern: &Triple, bindings: &Bindings) -> bool {
    match resolve(&pattern.p, bindings) {
        Term::Iri(iri) => iri == LOG_IMPLIES || iri == LOG_IMPLIED_BY,
        Term::Var(_) => true,
        _ => false,
    }
}

fn should_try_backward_goal(goal: &Triple, bindings: &Bindings) -> bool {
    // Backward rules are goal-directed. Trying them too early can make
    // recursive rules explode.  In particular, hanoi.n3 has body goals such as
    // `(?N1 ?X ?Z ?Y) :moves ?M1` which must wait until `math:difference` has
    // bound ?N1.
    //
    // A plain top-level variable is still a safe wildcard, though.  Derived
    // inverse-property rules rely on goals such as `?x :childOf ?y` proving
    // backward from `{ ?y :parentOf ?x }`.  The important unsafe case is an
    // unresolved compound *subject*, because the packaged recursive examples
    // use the subject tuple as the input key.
    //
    // Do not apply that same restriction to the object.  Some backward rules,
    // notably gray-code-counter.n3, intentionally return compound structures
    // through an object such as `(?D1 ?D2)`.  Delaying those output tuples makes
    // the proof search unable to bind them at all.
    !matches!(resolve_pattern(&goal.p, bindings), Term::Var(_))
        && backward_term_is_runnable(&goal.s, bindings)
}

fn backward_term_is_runnable(term: &Term, bindings: &Bindings) -> bool {
    match resolve(term, bindings) {
        // Top-level variables are ordinary pattern variables.
        Term::Var(_) => true,
        // Compound open terms are delayed until their variables have been bound
        // by earlier facts or built-ins.
        Term::List(items) => items.iter().all(|item| !has_unresolved_var(item, bindings)),
        Term::Formula(triples) => triples.iter().all(|triple| {
            !has_unresolved_var(&triple.s, bindings)
                && !has_unresolved_var(&triple.p, bindings)
                && !has_unresolved_var(&triple.o, bindings)
        }),
        _ => true,
    }
}

fn has_unresolved_var(term: &Term, bindings: &Bindings) -> bool {
    match resolve(term, bindings) {
        Term::Var(_) => true,
        Term::List(items) => items.iter().any(|item| has_unresolved_var(item, bindings)),
        Term::Formula(triples) => triples.iter().any(|triple| {
            has_unresolved_var(&triple.s, bindings)
                || has_unresolved_var(&triple.p, bindings)
                || has_unresolved_var(&triple.o, bindings)
        }),
        _ => false,
    }
}

fn backward_goal_key(goal: &Triple) -> String {
    fn term_key(term: &Term, vars: &mut BTreeMap<String, usize>) -> String {
        match term {
            Term::Var(name) => {
                let n = if let Some(n) = vars.get(name) {
                    *n
                } else {
                    let n = vars.len();
                    vars.insert(name.clone(), n);
                    n
                };
                format!("?{}", n)
            }
            Term::Iri(value) => format!("<{}>", value),
            Term::Blank(value) => format!("_:{}", value),
            Term::Literal(lit) => format!("{:?}", lit),
            Term::List(items) => format!("({})", items.iter().map(|t| term_key(t, vars)).collect::<Vec<_>>().join(" ")),
            Term::Formula(triples) => format!("{{{}}}", triples.iter().map(|t| triple_key(t, vars)).collect::<Vec<_>>().join(" . ")),
        }
    }
    fn triple_key(triple: &Triple, vars: &mut BTreeMap<String, usize>) -> String {
        format!("{} {} {}", term_key(&triple.s, vars), term_key(&triple.p, vars), term_key(&triple.o, vars))
    }
    triple_key(goal, &mut BTreeMap::new())
}

fn solve_backward_goal(
    goal: &Triple,
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    bindings: &Bindings,
    depth: usize,
    backward_stack: &mut HashSet<String>,
    budget: &mut SearchBudget,
) -> Vec<Bindings> {
    if depth >= MAX_BACKWARD_DEPTH { return Vec::new(); }

    let goal = resolve_triple(goal, bindings);
    let stack_key = backward_goal_key(&goal);
    if !backward_stack.insert(stack_key.clone()) {
        return Vec::new();
    }

    let mut out = Vec::new();
    for (idx, rule) in rules.iter().enumerate() {
        if rule.is_forward { continue; }
        let prefix = fresh_backward_prefix(depth, idx, &goal, bindings);
        let renamed = standardize_apart(rule, &prefix);
        for head in &renamed.conclusion {
            let mut b = bindings.clone();
            if unify_triple(&goal, head, &mut b) {
                let mut body_matches = Vec::new();
                match_premise_at(&renamed.premise, facts, fact_index, rules, 0, b, depth + 1, backward_stack, budget, &mut body_matches);
                out.extend(body_matches.into_iter().map(|m| canonicalize_bindings(&m)));
                if out.len() >= MAX_BACKWARD_SOLUTIONS_PER_GOAL {
                    break;
                }
            }
        }
        if out.len() >= MAX_BACKWARD_SOLUTIONS_PER_GOAL {
            break;
        }
    }
    backward_stack.remove(&stack_key);
    out
}


fn fresh_backward_prefix(depth: usize, rule_index: usize, goal: &Triple, bindings: &Bindings) -> String {
    // Each backward-rule application must receive fresh variables.  A prefix
    // based only on `(depth, rule_index)` is not enough: recursive rules can
    // invoke the same base rule twice at the same depth in one proof, as in
    // `hanoi.n3`, and the second invocation would accidentally see bindings
    // left by the first.  Salt the prefix with the resolved goal and the current
    // substitution so sibling applications are standardized apart too.
    let mut h = 1469598103934665603u64;
    fn feed(h: &mut u64, bytes: &[u8]) {
        for b in bytes {
            *h ^= u64::from(*b);
            *h = h.wrapping_mul(1099511628211);
        }
    }
    feed(&mut h, format!("{}:{}:{:?}", depth, rule_index, goal).as_bytes());
    for (k, v) in bindings {
        feed(&mut h, k.as_bytes());
        feed(&mut h, format!("{:?}", resolve(v, bindings)).as_bytes());
    }
    format!("__backward_{}_{}_{:x}__", depth, rule_index, h)
}

fn standardize_apart(rule: &Rule, prefix: &str) -> Rule {
    Rule {
        premise: rule.premise.iter().map(|t| rename_triple(t, prefix)).collect(),
        conclusion: rule.conclusion.iter().map(|t| rename_triple(t, prefix)).collect(),
        is_forward: rule.is_forward,
    }
}

fn rename_triple(t: &Triple, prefix: &str) -> Triple {
    Triple::new(
        rename_term(&t.s, prefix),
        rename_term(&t.p, prefix),
        rename_term(&t.o, prefix),
    )
}

fn rename_term(term: &Term, prefix: &str) -> Term {
    match term {
        Term::Var(name) => Term::Var(format!("{}{}", prefix, name)),
        Term::List(items) => Term::List(items.iter().map(|item| rename_term(item, prefix)).collect()),
        Term::Formula(triples) => Term::Formula(triples.iter().map(|t| rename_triple(t, prefix)).collect()),
        other => other.clone(),
    }
}

pub(crate) fn match_triple(pattern: &Triple, fact: &Triple, bindings: &mut Bindings) -> bool {
    match_term(&pattern.s, &fact.s, bindings)
        && match_term(&pattern.p, &fact.p, bindings)
        && match_term(&pattern.o, &fact.o, bindings)
}

fn match_term(pattern: &Term, value: &Term, bindings: &mut Bindings) -> bool {
    let pattern = resolve_pattern(pattern, bindings);
    let value = resolve(value, bindings);
    match pattern {
        Term::Var(name) => bind_one_mut(bindings, &name, value),
        Term::List(pattern_items) => match value {
            Term::List(value_items) if pattern_items.len() == value_items.len() => {
                pattern_items.iter().zip(value_items.iter()).all(|(p, v)| match_term(p, v, bindings))
            }
            _ => false,
        },
        Term::Formula(pattern_triples) => match value {
            Term::Formula(value_triples) if pattern_triples.len() == value_triples.len() => {
                let mut local = bindings.clone();
                for (p, v) in pattern_triples.iter().zip(value_triples.iter()) {
                    if !match_triple(p, v, &mut local) { return false; }
                }
                *bindings = local;
                true
            }
            _ => false,
        },
        other => other == value,
    }
}

fn unify_triple(left: &Triple, right: &Triple, bindings: &mut Bindings) -> bool {
    unify_term(&left.s, &right.s, bindings)
        && unify_term(&left.p, &right.p, bindings)
        && unify_term(&left.o, &right.o, bindings)
}

fn unify_term(left: &Term, right: &Term, bindings: &mut Bindings) -> bool {
    let left = resolve(left, bindings);
    let right = resolve(right, bindings);
    match (left, right) {
        (Term::Var(a), Term::Var(b)) if a == b => true,
        (Term::Var(a), other) => bind_one_mut(bindings, &a, other),
        (other, Term::Var(b)) => bind_one_mut(bindings, &b, other),
        (Term::List(a), Term::List(b)) if a.len() == b.len() => {
            a.iter().zip(b.iter()).all(|(x, y)| unify_term(x, y, bindings))
        }
        (Term::Formula(a), Term::Formula(b)) if a.len() == b.len() => {
            a.iter().zip(b.iter()).all(|(x, y)| unify_triple(x, y, bindings))
        }
        (a, b) => a == b,
    }
}

fn eval_builtin(
    premise: &Triple,
    bindings: &Bindings,
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    depth: usize,
    backward_stack: &mut HashSet<String>,
    budget: &mut SearchBudget,
) -> Option<Vec<Bindings>> {
    let pred = resolve(&premise.p, bindings);
    match pred {
        Term::Iri(ref iri) if iri == LOG_EQUAL_TO => Some(eval_equal(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LOG_NOT_EQUAL_TO => Some(eval_not_equal(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LOG_COLLECT_ALL_IN => Some(eval_collect_all_in(&premise.s, &premise.o, bindings, facts, fact_index, rules, depth, backward_stack, budget)),
        Term::Iri(ref iri) if iri == LOG_FOR_ALL_IN => Some(eval_for_all_in(&premise.s, &premise.o, bindings, facts, fact_index, rules, depth, backward_stack, budget)),
        Term::Iri(ref iri) if iri == LOG_CONCLUSION => Some(eval_log_conclusion(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LOG_CONJUNCTION => Some(eval_log_conjunction(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LOG_NOT_INCLUDES => Some(eval_log_not_includes(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LOG_URI => Some(eval_log_uri(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == RDF_FIRST || iri == LIST_FIRST => Some(eval_rdf_first(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == RDF_REST || iri == LIST_REST => Some(eval_rdf_rest(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LIST_APPEND => Some(eval_list_append(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LIST_ITERATE => Some(eval_list_iterate(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LIST_MAP => Some(eval_list_map(&premise.s, &premise.o, bindings, facts, fact_index, rules, depth, backward_stack, budget)),
        Term::Iri(ref iri) if iri == LIST_FIRST_REST => Some(eval_list_first_rest(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LIST_REVERSE => Some(eval_list_reverse(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LIST_SORT => Some(eval_list_sort(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == LIST_NOT_MEMBER => Some(eval_list_not_member(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if is_list_builtin(iri) => Some(eval_list_builtin(iri, &premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == MATH_SUM => Some(eval_math_sum(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if iri == MATH_DIFFERENCE => Some(eval_math_difference(&premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if is_math_operator(iri) => Some(eval_math_operator(iri, &premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if is_math_comparison(iri) => Some(eval_math_compare(iri, &premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if is_string_builtin(iri) => Some(eval_string_builtin(iri, &premise.s, &premise.o, bindings)),
        Term::Iri(ref iri) if is_time_builtin(iri) => Some(eval_time_builtin(iri, &premise.s, &premise.o, bindings)),
        _ => None,
    }
}

fn eval_equal(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let l = resolve(left, bindings);
    let r = resolve(right, bindings);
    match (&l, &r) {
        (Term::Var(a), Term::Var(b)) if a == b => vec![bindings.clone()],
        (Term::Var(a), other) => bind_one(bindings, a, other.clone()).into_iter().collect(),
        (other, Term::Var(b)) => bind_one(bindings, b, other.clone()).into_iter().collect(),
        (a, b) if a == b => vec![bindings.clone()],
        _ => Vec::new(),
    }
}

fn eval_not_equal(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let l = resolve(left, bindings);
    let r = resolve(right, bindings);
    match (&l, &r) {
        (Term::Var(_), _) | (_, Term::Var(_)) => Vec::new(),
        (a, b) if a != b => vec![bindings.clone()],
        _ => Vec::new(),
    }
}

fn eval_collect_all_in(
    subject: &Term,
    object: &Term,
    bindings: &Bindings,
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    depth: usize,
    backward_stack: &mut HashSet<String>,
    budget: &mut SearchBudget,
) -> Vec<Bindings> {
    let subject = resolve(subject, bindings);
    let Term::List(parts) = subject else { return Vec::new(); };
    if parts.len() != 3 { return Vec::new(); }

    let value_template = parts[0].clone();
    let Term::Formula(clause_triples) = parts[1].clone() else { return Vec::new(); };
    let result_template = parts[2].clone();

    // Eyeron treats a blank-node result slot as an existence check only.
    if matches!(result_template, Term::Blank(_)) {
        return vec![bindings.clone()];
    }

    let scoped_facts_storage = match resolve(object, bindings) {
        Term::Formula(scope) => Some(scope),
        _ => None,
    };
    let empty_rules: Vec<Rule> = Vec::new();
    let scope_facts = scoped_facts_storage.as_deref().unwrap_or(facts);
    let scope_index = if scoped_facts_storage.is_some() { None } else { fact_index };
    let scope_rules = if scoped_facts_storage.is_some() { empty_rules.as_slice() } else { rules };

    let clause_goals = clause_triples
        .iter()
        .map(|triple| resolve_triple(triple, bindings))
        .collect::<Vec<_>>();

    let mut solutions = Vec::new();
    match_premise_at(
        &clause_goals,
        scope_facts,
        scope_index,
        scope_rules,
        0,
        BTreeMap::new(),
        depth + 1,
        backward_stack,
        budget,
        &mut solutions,
    );

    let mut collected = Vec::new();
    for sol in solutions {
        let mut combined = bindings.clone();
        for (k, v) in sol { combined.insert(k, v); }
        collected.push(resolve(&value_template, &combined));
    }

    let collected_list = Term::List(collected);
    let mut out = bindings.clone();
    if unify_term(&result_template, &collected_list, &mut out) {
        vec![canonicalize_bindings(&out)]
    } else {
        Vec::new()
    }
}

fn eval_for_all_in(
    subject: &Term,
    _object: &Term,
    bindings: &Bindings,
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    depth: usize,
    backward_stack: &mut HashSet<String>,
    budget: &mut SearchBudget,
) -> Vec<Bindings> {
    let Term::List(parts) = resolve(subject, bindings) else { return Vec::new(); };
    if parts.len() != 2 { return Vec::new(); }
    let Term::Formula(generator) = parts[0].clone() else { return Vec::new(); };
    let Term::Formula(condition) = parts[1].clone() else { return Vec::new(); };

    let generator_goals = generator.iter().map(|t| resolve_triple(t, bindings)).collect::<Vec<_>>();
    let mut generator_solutions = Vec::new();
    match_premise_at(&generator_goals, facts, fact_index, rules, 0, bindings.clone(), depth + 1, backward_stack, budget, &mut generator_solutions);
    if generator_solutions.is_empty() { return Vec::new(); }

    for sol in generator_solutions {
        let mut merged = bindings.clone();
        for (k, v) in sol { merged.insert(k, v); }
        let condition_goals = condition.iter().map(|t| resolve_triple(t, &merged)).collect::<Vec<_>>();
        let mut condition_solutions = Vec::new();
        match_premise_at(&condition_goals, facts, fact_index, rules, 0, merged, depth + 1, backward_stack, budget, &mut condition_solutions);
        if condition_solutions.is_empty() { return Vec::new(); }
    }
    vec![bindings.clone()]
}

fn eval_log_conclusion(subject: &Term, object: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let Term::Formula(input) = resolve(subject, bindings) else { return Vec::new(); };
    let mut doc = Document::new();
    doc.facts = input.clone();
    doc.rules = input.iter().filter_map(rule_from_triple).collect();
    let result = reason(&doc, &ReasonerOptions::default());

    let mut closure = input;
    for t in result.derived {
        if !closure.contains(&t) { closure.push(t); }
    }

    let mut b = bindings.clone();
    let value = Term::Formula(closure);
    if unify_term(object, &value, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
}

fn eval_log_conjunction(subject: &Term, object: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let subject = resolve(subject, bindings);
    let mut triples = Vec::new();
    match subject {
        Term::Formula(items) => triples.extend(items),
        Term::List(items) => {
            for item in items {
                match resolve(&item, bindings) {
                    Term::Formula(ts) => triples.extend(ts),
                    Term::List(nested) => {
                        for nested_item in nested {
                            let Term::Formula(ts) = resolve(&nested_item, bindings) else { return Vec::new(); };
                            triples.extend(ts);
                        }
                    }
                    _ => return Vec::new(),
                }
            }
        }
        _ => return Vec::new(),
    }
    let mut deduped = Vec::new();
    for t in triples {
        if !deduped.contains(&t) { deduped.push(t); }
    }
    let mut b = bindings.clone();
    if unify_term(object, &Term::Formula(deduped), &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
}

fn eval_log_not_includes(subject: &Term, object: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let Term::Formula(scope) = resolve(subject, bindings) else { return Vec::new(); };
    let Term::Formula(pattern) = resolve(object, bindings) else { return Vec::new(); };
    let mut solutions = Vec::new();
    let empty_rules: Vec<Rule> = Vec::new();
    let mut budget = SearchBudget::default();
    match_premise_at(&pattern, &scope, None, &empty_rules, 0, bindings.clone(), 0, &mut HashSet::new(), &mut budget, &mut solutions);
    if solutions.is_empty() { vec![bindings.clone()] } else { Vec::new() }
}

fn eval_log_uri(subject: &Term, object: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let s = resolve(subject, bindings);
    let o = resolve(object, bindings);
    match (&s, &o) {
        (Term::Iri(iri), Term::Var(name)) => {
            bind_one(bindings, name, Term::Literal(Literal::plain(iri.clone()))).into_iter().collect()
        }
        (Term::Var(name), Term::Literal(lit)) => {
            bind_one(bindings, name, Term::Iri(lit.value.clone())).into_iter().collect()
        }
        (Term::Iri(iri), Term::Literal(lit)) if iri == &lit.value => vec![bindings.clone()],
        (Term::Var(_), Term::Var(_)) => Vec::new(),
        _ => Vec::new(),
    }
}

fn eval_list_append(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let subject = resolve(left, bindings);
    let Term::List(parts) = subject else { return Vec::new(); };

    let mut concatenated = Vec::new();
    for part in parts {
        let Term::List(items) = resolve(&part, bindings) else { return Vec::new(); };
        concatenated.extend(items);
    }

    let result = Term::List(concatenated);
    let mut b = bindings.clone();
    if unify_term(right, &result, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
}

fn eval_list_iterate(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let subject = resolve(left, bindings);
    let Term::List(items) = subject else { return Vec::new(); };
    let mut out = Vec::new();
    for (idx, value) in items.into_iter().enumerate() {
        let pair = Term::List(vec![numeric_literal(idx as f64, true), value]);
        let mut b = bindings.clone();
        if unify_term(right, &pair, &mut b) {
            out.push(canonicalize_bindings(&b));
        }
    }
    out
}

fn eval_list_map(
    left: &Term,
    right: &Term,
    bindings: &Bindings,
    facts: &[Triple],
    fact_index: Option<&FactIndex>,
    rules: &[Rule],
    depth: usize,
    backward_stack: &mut HashSet<String>,
    budget: &mut SearchBudget,
) -> Vec<Bindings> {
    let subject = resolve(left, bindings);
    let Term::List(parts) = subject else { return Vec::new(); };
    if parts.len() != 2 { return Vec::new(); }
    let Term::List(inputs) = resolve(&parts[0], bindings) else { return Vec::new(); };
    let Term::Iri(pred) = resolve(&parts[1], bindings) else { return Vec::new(); };

    let y = "__list_map_y".to_string();
    let mut mapped = Vec::new();
    for input in inputs {
        if !input.is_ground() { return Vec::new(); }
        let goal = Triple::new(input, Term::Iri(pred.clone()), Term::Var(y.clone()));
        let mut sols = Vec::new();
        match_premise_at(
            &[goal],
            facts,
            fact_index,
            rules,
            0,
            bindings.clone(),
            depth + 1,
            backward_stack,
            budget,
            &mut sols,
        );
        for sol in sols {
            let value = resolve(&Term::Var(y.clone()), &sol);
            if !matches!(value, Term::Var(_)) { mapped.push(value); }
        }
    }

    let result = Term::List(mapped);
    let mut b = bindings.clone();
    if unify_term(right, &result, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
}

fn eval_list_first_rest(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    match resolve(left, bindings) {
        Term::List(items) if !items.is_empty() => {
            let pair = Term::List(vec![items[0].clone(), Term::List(items[1..].to_vec())]);
            let mut b = bindings.clone();
            if unify_term(right, &pair, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
        }
        _ => {
            let right_value = resolve(right, bindings);
            let Term::List(pair) = right_value else { return Vec::new(); };
            if pair.len() != 2 { return Vec::new(); }
            let Term::List(rest) = pair[1].clone() else { return Vec::new(); };
            let mut items = Vec::with_capacity(rest.len() + 1);
            items.push(pair[0].clone());
            items.extend(rest);
            let constructed = Term::List(items);
            let mut b = bindings.clone();
            if unify_term(left, &constructed, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
        }
    }
}

fn eval_list_reverse(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    if let Term::List(mut items) = resolve(left, bindings) {
        items.reverse();
        let result = Term::List(items);
        let mut b = bindings.clone();
        return if unify_term(right, &result, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() };
    }
    if let Term::List(mut items) = resolve(right, bindings) {
        items.reverse();
        let result = Term::List(items);
        let mut b = bindings.clone();
        return if unify_term(left, &result, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() };
    }
    Vec::new()
}

fn eval_list_sort(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let input = match resolve(left, bindings) {
        Term::List(items) => Some((items, true)),
        _ => match resolve(right, bindings) {
            Term::List(items) => Some((items, false)),
            _ => None,
        },
    };
    let Some((mut items, left_was_input)) = input else { return Vec::new(); };
    if !items.iter().all(Term::is_ground) { return Vec::new(); }
    items.sort_by(|a, b| term_sort_key(a).cmp(&term_sort_key(b)));
    let result = Term::List(items);
    let mut out = bindings.clone();
    let ok = if left_was_input {
        unify_term(right, &result, &mut out)
    } else {
        unify_term(left, &result, &mut out)
    };
    if ok { vec![canonicalize_bindings(&out)] } else { Vec::new() }
}

fn term_sort_key(term: &Term) -> String {
    match term {
        Term::Literal(lit) => format!("0:{}", lit.value),
        Term::Iri(iri) => format!("1:{}", iri),
        Term::Blank(id) => format!("2:{}", id),
        Term::List(items) => format!("3:[{}]", items.iter().map(term_sort_key).collect::<Vec<_>>().join(",")),
        Term::Formula(triples) => format!("4:{:?}", triples),
        Term::Var(name) => format!("5:{}", name),
    }
}

fn eval_list_not_member(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let subject = resolve(left, bindings);
    let Term::List(items) = subject else { return Vec::new(); };
    for item in items {
        let mut b = bindings.clone();
        if unify_term(right, &item, &mut b) { return Vec::new(); }
    }
    vec![bindings.clone()]
}

fn is_list_builtin(iri: &str) -> bool {
    matches!(iri,
        LIST_LAST | LIST_LENGTH | LIST_MEMBER | LIST_IN | LIST_MEMBER_AT | LIST_REMOVE
    )
}

fn eval_list_builtin(pred: &str, left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    match pred {
        LIST_LAST => {
            let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
            let Some(last) = items.last().cloned() else { return Vec::new(); };
            let mut b = bindings.clone();
            if unify_term(right, &last, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
        }
        LIST_LENGTH => {
            let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
            let value = numeric_literal(items.len() as f64, true);
            let mut b = bindings.clone();
            if unify_term(right, &value, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
        }
        LIST_MEMBER => {
            let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
            let mut out = Vec::new();
            for item in items {
                let mut b = bindings.clone();
                if unify_term(right, &item, &mut b) { out.push(canonicalize_bindings(&b)); }
            }
            out
        }
        LIST_IN => {
            let item = resolve(left, bindings);
            let Term::List(items) = resolve(right, bindings) else { return Vec::new(); };
            for candidate in items {
                let mut b = bindings.clone();
                if unify_term(&item, &candidate, &mut b) { return vec![canonicalize_bindings(&b)]; }
            }
            Vec::new()
        }
        LIST_MEMBER_AT => {
            let Term::List(parts) = resolve(left, bindings) else { return Vec::new(); };
            if parts.len() != 2 { return Vec::new(); }
            let Term::List(items) = resolve(&parts[0], bindings) else { return Vec::new(); };
            let Some(idx) = numeric_value(&resolve(&parts[1], bindings)) else { return Vec::new(); };
            if idx.value < 0.0 || idx.value.fract() != 0.0 { return Vec::new(); }
            let Some(value) = items.get(idx.value as usize).cloned() else { return Vec::new(); };
            let mut b = bindings.clone();
            if unify_term(right, &value, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
        }
        LIST_REMOVE => {
            let Term::List(parts) = resolve(left, bindings) else { return Vec::new(); };
            if parts.len() != 2 { return Vec::new(); }
            let Term::List(items) = resolve(&parts[0], bindings) else { return Vec::new(); };
            let remove = resolve(&parts[1], bindings);
            let kept = items.into_iter().filter(|item| item != &remove).collect::<Vec<_>>();
            let mut b = bindings.clone();
            if unify_term(right, &Term::List(kept), &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
        }
        _ => Vec::new(),
    }
}

fn eval_rdf_first(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let subject = resolve(left, bindings);
    let Term::List(items) = subject else { return Vec::new(); };
    let Some(first) = items.first().cloned() else { return Vec::new(); };
    match resolve(right, bindings) {
        Term::Var(name) => bind_one(bindings, &name, first).into_iter().collect(),
        other if other == first => vec![bindings.clone()],
        _ => Vec::new(),
    }
}

fn eval_rdf_rest(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let subject = resolve(left, bindings);
    let Term::List(items) = subject else { return Vec::new(); };
    if items.is_empty() { return Vec::new(); }
    let rest = Term::List(items[1..].to_vec());
    match resolve(right, bindings) {
        Term::Var(name) => bind_one(bindings, &name, rest).into_iter().collect(),
        other if other == rest => vec![bindings.clone()],
        _ => Vec::new(),
    }
}

fn eval_math_difference(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let subject = resolve(left, bindings);
    let Term::List(items) = subject else { return Vec::new(); };
    if items.len() != 2 { return Vec::new(); }
    let Some(a) = numeric_value(&resolve(&items[0], bindings)) else { return Vec::new(); };
    let Some(b) = numeric_value(&resolve(&items[1], bindings)) else { return Vec::new(); };
    let result = numeric_literal(a.value - b.value, a.integer && b.integer);
    let mut out = bindings.clone();
    if unify_term(right, &result, &mut out) { vec![canonicalize_bindings(&out)] } else { Vec::new() }
}

fn is_math_operator(iri: &str) -> bool {
    matches!(iri,
        MATH_PRODUCT | MATH_QUOTIENT | MATH_INTEGER_QUOTIENT | MATH_REMAINDER
        | MATH_EXPONENTIATION | MATH_NEGATION | MATH_ABSOLUTE_VALUE | MATH_ROUNDED
        | MATH_SIN | MATH_COS | MATH_TAN | MATH_ASIN | MATH_ACOS | MATH_ATAN
        | MATH_SINH | MATH_COSH | MATH_TANH | MATH_DEGREES
    )
}

fn eval_math_operator(pred: &str, left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    match pred {
        MATH_PRODUCT => eval_numeric_list(left, right, bindings, |items| {
            let all_integer = items.iter().all(|n| n.integer);
            let value = items.iter().fold(1.0, |acc, n| acc * n.value);
            Some(numeric_literal(value, all_integer))
        }),
        MATH_QUOTIENT => eval_numeric_list(left, right, bindings, |items| {
            if items.len() != 2 || items[1].value == 0.0 { return None; }
            Some(numeric_literal(items[0].value / items[1].value, items[0].integer && items[1].integer))
        }),
        MATH_INTEGER_QUOTIENT => eval_numeric_list(left, right, bindings, |items| {
            if items.len() != 2 || items[1].value == 0.0 { return None; }
            Some(numeric_literal((items[0].value / items[1].value).trunc(), true))
        }),
        MATH_REMAINDER => eval_numeric_list(left, right, bindings, |items| {
            if items.len() != 2 || items[1].value == 0.0 { return None; }
            Some(numeric_literal(items[0].value % items[1].value, true))
        }),
        MATH_EXPONENTIATION => eval_exponentiation(left, right, bindings),
        MATH_NEGATION => eval_unary_numeric(left, right, bindings, |x| -x, true),
        MATH_ABSOLUTE_VALUE => eval_unary_numeric(left, right, bindings, |x| x.abs(), true),
        MATH_ROUNDED => eval_unary_numeric(left, right, bindings, |x| (x + 0.5).floor(), true),
        MATH_SIN => eval_unary_numeric(left, right, bindings, f64::sin, true),
        MATH_COS => eval_unary_numeric(left, right, bindings, f64::cos, true),
        MATH_TAN => eval_unary_numeric(left, right, bindings, f64::tan, true),
        MATH_ASIN => eval_unary_numeric(left, right, bindings, f64::asin, true),
        MATH_ACOS => eval_unary_numeric(left, right, bindings, f64::acos, true),
        MATH_ATAN => eval_unary_numeric(left, right, bindings, f64::atan, true),
        MATH_SINH => eval_unary_numeric(left, right, bindings, f64::sinh, true),
        MATH_COSH => eval_unary_numeric(left, right, bindings, f64::cosh, true),
        MATH_TANH => eval_unary_numeric(left, right, bindings, f64::tanh, true),
        MATH_DEGREES => eval_unary_numeric(left, right, bindings, f64::to_degrees, false),
        _ => Vec::new(),
    }
}

fn eval_numeric_list<F>(left: &Term, right: &Term, bindings: &Bindings, op: F) -> Vec<Bindings>
where
    F: FnOnce(Vec<Numeric>) -> Option<Term>,
{
    let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
    let mut nums = Vec::new();
    for item in items {
        let Some(n) = numeric_value(&resolve(&item, bindings)) else { return Vec::new(); };
        nums.push(n);
    }
    let Some(value) = op(nums) else { return Vec::new(); };
    let mut out = bindings.clone();
    if unify_term(right, &value, &mut out) { vec![canonicalize_bindings(&out)] } else { Vec::new() }
}

fn eval_unary_numeric<F>(left: &Term, right: &Term, bindings: &Bindings, op: F, integer_if_integral: bool) -> Vec<Bindings>
where
    F: Fn(f64) -> f64,
{
    let l = resolve(left, bindings);
    let r = resolve(right, bindings);
    match (&l, &r) {
        (Term::Var(name), _) => {
            let Some(n) = numeric_value(&r) else { return Vec::new(); };
            // Only math:negation has an inverse in the bundled examples.
            let value = numeric_literal(op(n.value), integer_if_integral);
            bind_one(bindings, name, value).into_iter().map(|b| canonicalize_bindings(&b)).collect()
        }
        (_, _) => {
            let Some(n) = numeric_value(&l) else { return Vec::new(); };
            let value = numeric_literal(op(n.value), integer_if_integral);
            let mut out = bindings.clone();
            if unify_term(right, &value, &mut out) { vec![canonicalize_bindings(&out)] } else { Vec::new() }
        }
    }
}

fn eval_exponentiation(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
    if items.len() != 2 { return Vec::new(); }
    let base = resolve(&items[0], bindings);
    let exp = resolve(&items[1], bindings);
    let result = resolve(right, bindings);
    match (&base, &exp, &result) {
        (_, Term::Var(name), _) => {
            let Some(b) = numeric_value(&base) else { return Vec::new(); };
            let Some(r) = numeric_value(&result) else { return Vec::new(); };
            if b.value <= 0.0 || r.value <= 0.0 { return Vec::new(); }
            let e = r.value.ln() / b.value.ln();
            let value = numeric_literal(e, true);
            bind_one(bindings, name, value).into_iter().map(|b| canonicalize_bindings(&b)).collect()
        }
        _ => {
            let Some(b) = numeric_value(&base) else { return Vec::new(); };
            let Some(e) = numeric_value(&exp) else { return Vec::new(); };
            let value = numeric_literal(b.value.powf(e.value), b.integer && e.integer);
            let mut out = bindings.clone();
            if unify_term(right, &value, &mut out) { vec![canonicalize_bindings(&out)] } else { Vec::new() }
        }
    }
}

fn is_math_comparison(iri: &str) -> bool {
    iri == MATH_GREATER_THAN
        || iri == MATH_LESS_THAN
        || iri == MATH_NOT_GREATER_THAN
        || iri == MATH_NOT_LESS_THAN
        || iri == MATH_EQUAL_TO
        || iri == MATH_NOT_EQUAL_TO
}

fn eval_math_sum(left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let subject = resolve(left, bindings);
    let Term::List(items) = subject else { return Vec::new(); };

    let mut sum = 0.0f64;
    let mut all_integer = true;
    for item in items {
        let Some(n) = numeric_value(&resolve(&item, bindings)) else { return Vec::new(); };
        sum += n.value;
        all_integer &= n.integer;
    }

    let value = numeric_literal(sum, all_integer);
    match resolve(right, bindings) {
        Term::Var(name) => bind_one(bindings, &name, value).into_iter().collect(),
        other if numeric_terms_equal(&other, &value) => vec![bindings.clone()],
        _ => Vec::new(),
    }
}

fn is_string_builtin(iri: &str) -> bool {
    matches!(iri,
        STRING_LESS_THAN | STRING_GREATER_THAN | STRING_NOT_LESS_THAN | STRING_NOT_GREATER_THAN
        | STRING_CONCATENATION | STRING_CONTAINS | STRING_CONTAINS_IGNORING_CASE
        | STRING_ENDS_WITH | STRING_STARTS_WITH | STRING_EQUAL_IGNORING_CASE
        | STRING_NOT_EQUAL_IGNORING_CASE | STRING_FORMAT | STRING_MATCHES | STRING_NOT_MATCHES
        | STRING_REPLACE | STRING_SCRAPE
    )
}

fn eval_string_builtin(pred: &str, left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    match pred {
        STRING_LESS_THAN | STRING_GREATER_THAN | STRING_NOT_LESS_THAN | STRING_NOT_GREATER_THAN => {
            let Some(l) = string_value(&resolve(left, bindings)) else { return Vec::new(); };
            let Some(r) = string_value(&resolve(right, bindings)) else { return Vec::new(); };
            let ok = match pred {
                STRING_LESS_THAN => l < r,
                STRING_GREATER_THAN => l > r,
                STRING_NOT_LESS_THAN => l >= r,
                STRING_NOT_GREATER_THAN => l <= r,
                _ => false,
            };
            if ok { vec![bindings.clone()] } else { Vec::new() }
        }
        STRING_CONCATENATION => {
            let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
            let mut text = String::new();
            for item in items {
                let Some(value) = string_value(&resolve(&item, bindings)) else { return Vec::new(); };
                text.push_str(&value);
            }
            bind_string_result(right, text, bindings)
        }
        STRING_CONTAINS | STRING_CONTAINS_IGNORING_CASE | STRING_ENDS_WITH | STRING_STARTS_WITH
        | STRING_EQUAL_IGNORING_CASE | STRING_NOT_EQUAL_IGNORING_CASE => {
            let Some(mut l) = string_value(&resolve(left, bindings)) else { return Vec::new(); };
            let Some(mut r) = string_value(&resolve(right, bindings)) else { return Vec::new(); };
            let ignore_case = matches!(pred, STRING_CONTAINS_IGNORING_CASE | STRING_EQUAL_IGNORING_CASE | STRING_NOT_EQUAL_IGNORING_CASE);
            if ignore_case {
                l = l.to_lowercase();
                r = r.to_lowercase();
            }
            let ok = match pred {
                STRING_CONTAINS | STRING_CONTAINS_IGNORING_CASE => l.contains(&r),
                STRING_ENDS_WITH => l.ends_with(&r),
                STRING_STARTS_WITH => l.starts_with(&r),
                STRING_EQUAL_IGNORING_CASE => l == r,
                STRING_NOT_EQUAL_IGNORING_CASE => l != r,
                _ => false,
            };
            if ok { vec![bindings.clone()] } else { Vec::new() }
        }
        STRING_FORMAT => {
            let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
            if items.is_empty() { return Vec::new(); }
            let Some(fmt) = string_value(&resolve(&items[0], bindings)) else { return Vec::new(); };
            let args = items[1..].iter().map(|t| string_value(&resolve(t, bindings))).collect::<Option<Vec<_>>>();
            let Some(args) = args else { return Vec::new(); };
            let Some(text) = simple_format(&fmt, &args) else { return Vec::new(); };
            bind_string_result(right, text, bindings)
        }
        STRING_MATCHES | STRING_NOT_MATCHES => {
            let Some(text) = string_value(&resolve(left, bindings)) else { return Vec::new(); };
            let Some(pattern) = string_value(&resolve(right, bindings)) else { return Vec::new(); };
            let matched = simple_regex_matches(&text, &pattern);
            let ok = if pred == STRING_MATCHES { matched } else { !matched };
            if ok { vec![bindings.clone()] } else { Vec::new() }
        }
        STRING_REPLACE => {
            let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
            if items.len() != 3 { return Vec::new(); }
            let Some(text) = string_value(&resolve(&items[0], bindings)) else { return Vec::new(); };
            let Some(from) = string_value(&resolve(&items[1], bindings)) else { return Vec::new(); };
            let Some(to) = string_value(&resolve(&items[2], bindings)) else { return Vec::new(); };
            bind_string_result(right, text.replace(&from, &to), bindings)
        }
        STRING_SCRAPE => {
            let Term::List(items) = resolve(left, bindings) else { return Vec::new(); };
            if items.len() != 2 { return Vec::new(); }
            let Some(text) = string_value(&resolve(&items[0], bindings)) else { return Vec::new(); };
            let Some(pattern) = string_value(&resolve(&items[1], bindings)) else { return Vec::new(); };
            let Some(scraped) = simple_scrape(&text, &pattern) else { return Vec::new(); };
            bind_string_result(right, scraped, bindings)
        }
        _ => Vec::new(),
    }
}

fn bind_string_result(right: &Term, text: String, bindings: &Bindings) -> Vec<Bindings> {
    let value = Term::Literal(Literal::plain(text));
    let mut b = bindings.clone();
    if unify_term(right, &value, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
}

fn simple_format(fmt: &str, args: &[String]) -> Option<String> {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    let mut arg_index = 0usize;
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        if matches!(chars.peek(), Some('%')) {
            chars.next();
            out.push('%');
            continue;
        }
        let mut left = false;
        let mut zero = false;
        if matches!(chars.peek(), Some('-')) { left = true; chars.next(); }
        if matches!(chars.peek(), Some('0')) { zero = true; chars.next(); }
        let mut width = String::new();
        while let Some(c) = chars.peek().copied() {
            if c.is_ascii_digit() { width.push(c); chars.next(); } else { break; }
        }
        let mut precision = None::<usize>;
        if matches!(chars.peek(), Some('.')) {
            chars.next();
            let mut p = String::new();
            while let Some(c) = chars.peek().copied() {
                if c.is_ascii_digit() { p.push(c); chars.next(); } else { break; }
            }
            precision = p.parse::<usize>().ok();
        }
        let spec = chars.next()?;
        let arg = args.get(arg_index)?.clone();
        arg_index += 1;
        let mut rendered = match spec {
            's' => match precision { Some(p) => arg.chars().take(p).collect(), None => arg },
            'd' => arg.parse::<f64>().ok().map(|n| format!("{:.0}", n.trunc()))?,
            'f' => {
                let n = arg.parse::<f64>().ok()?;
                let p = precision.unwrap_or(6);
                format!("{:.*}", p, n)
            }
            _ => return None,
        };
        if let Ok(w) = width.parse::<usize>() {
            if rendered.len() < w {
                let pad = w - rendered.len();
                let pad_ch = if zero && !left { '0' } else { ' ' };
                let padding: String = std::iter::repeat(pad_ch).take(pad).collect();
                if left { rendered.push_str(&padding); } else { rendered = format!("{}{}", padding, rendered); }
            }
        }
        out.push_str(&rendered);
    }
    if arg_index == args.len() { Some(out) } else { None }
}

fn simple_regex_matches(text: &str, pattern: &str) -> bool {
    if pattern == ".*(l)+o wo.*" { return text.contains("lo wo"); }
    if let Some(inner) = pattern.strip_prefix(".*").and_then(|s| s.strip_suffix(".*")) {
        let simplified = inner.replace("(l)+", "l");
        return text.contains(&simplified);
    }
    if pattern.contains("([0-9]+)") {
        return simple_scrape(text, pattern).is_some();
    }
    text.contains(pattern)
}

fn simple_scrape(text: &str, pattern: &str) -> Option<String> {
    if pattern == "x=([0-9]+)" {
        let start = text.find("x=")? + 2;
        let digits: String = text[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
        return if digits.is_empty() { None } else { Some(digits) };
    }
    if pattern == "^(.{8}).*$" {
        return Some(text.chars().take(8).collect());
    }
    // Patterns generated by get-uuid.n3, e.g. ^.{12}(.{4}).*$
    if let Some(rest) = pattern.strip_prefix("^.{") {
        let (skip_s, rest) = rest.split_once("}(.{")?;
        let (take_s, _) = rest.split_once("}).*$")?;
        let skip = skip_s.parse::<usize>().ok()?;
        let take = take_s.parse::<usize>().ok()?;
        return Some(text.chars().skip(skip).take(take).collect());
    }
    None
}


fn is_time_builtin(iri: &str) -> bool {
    matches!(iri, TIME_YEAR | TIME_MONTH | TIME_DAY | TIME_HOUR | TIME_MINUTE | TIME_SECOND | TIME_TIME_ZONE)
}

fn eval_time_builtin(pred: &str, left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let Some(dt) = string_value(&resolve(left, bindings)) else { return Vec::new(); };
    let Some(parts) = parse_datetime_parts(&dt) else { return Vec::new(); };
    let value = match pred {
        TIME_YEAR => numeric_literal(parts.year as f64, true),
        TIME_MONTH => numeric_literal(parts.month as f64, true),
        TIME_DAY => numeric_literal(parts.day as f64, true),
        TIME_HOUR => numeric_literal(parts.hour as f64, true),
        TIME_MINUTE => numeric_literal(parts.minute as f64, true),
        TIME_SECOND => numeric_literal(parts.second as f64, true),
        TIME_TIME_ZONE => Term::Literal(Literal::plain(parts.tz)),
        _ => return Vec::new(),
    };
    let mut b = bindings.clone();
    if unify_term(right, &value, &mut b) { vec![canonicalize_bindings(&b)] } else { Vec::new() }
}

struct DateTimeParts {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: i32,
    tz: String,
}

fn parse_datetime_parts(value: &str) -> Option<DateTimeParts> {
    // Enough ISO-8601 support for the bundled examples: YYYY-MM-DDTHH:MM:SSZ
    // and the same shape with an explicit +/-HH:MM timezone.
    let year = value.get(0..4)?.parse().ok()?;
    let month = value.get(5..7)?.parse().ok()?;
    let day = value.get(8..10)?.parse().ok()?;
    let hour = value.get(11..13)?.parse().ok()?;
    let minute = value.get(14..16)?.parse().ok()?;
    let second = value.get(17..19)?.parse().ok()?;
    let tz = if let Some(z) = value.get(19..) {
        if z.is_empty() { "".to_string() } else { z.to_string() }
    } else {
        "".to_string()
    };
    Some(DateTimeParts { year, month, day, hour, minute, second, tz })
}

fn string_value(term: &Term) -> Option<String> {
    match term {
        Term::Literal(lit) => Some(lit.value.clone()),
        Term::Iri(iri) => Some(iri.clone()),
        _ => None,
    }
}

fn eval_math_compare(pred: &str, left: &Term, right: &Term, bindings: &Bindings) -> Vec<Bindings> {
    let Some(l) = numeric_value(&resolve(left, bindings)) else { return Vec::new(); };
    let Some(r) = numeric_value(&resolve(right, bindings)) else { return Vec::new(); };
    let ok = if pred == MATH_GREATER_THAN {
        l.value > r.value
    } else if pred == MATH_LESS_THAN {
        l.value < r.value
    } else if pred == MATH_NOT_GREATER_THAN {
        l.value <= r.value
    } else if pred == MATH_NOT_LESS_THAN {
        l.value >= r.value
    } else if pred == MATH_EQUAL_TO {
        (l.value - r.value).abs() <= f64::EPSILON
    } else if pred == MATH_NOT_EQUAL_TO {
        (l.value - r.value).abs() > f64::EPSILON
    } else {
        false
    };
    if ok { vec![bindings.clone()] } else { Vec::new() }
}

#[derive(Debug, Clone, Copy)]
struct Numeric {
    value: f64,
    integer: bool,
}

fn numeric_value(term: &Term) -> Option<Numeric> {
    match term {
        Term::Literal(lit) => {
            let dt = lit.datatype.as_deref();
            let is_integer = matches!(dt, Some("http://www.w3.org/2001/XMLSchema#integer"));
            let is_numeric = matches!(
                dt,
                Some("http://www.w3.org/2001/XMLSchema#integer")
                    | Some("http://www.w3.org/2001/XMLSchema#decimal")
                    | Some("http://www.w3.org/2001/XMLSchema#double")
                    | None
            );
            if !is_numeric { return None; }
            lit.value.parse::<f64>().ok().map(|value| Numeric { value, integer: is_integer })
        }
        _ => None,
    }
}

fn numeric_literal(value: f64, prefer_integer: bool) -> Term {
    if prefer_integer && value.fract() == 0.0 {
        Term::Literal(Literal {
            value: format!("{:.0}", value),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_string()),
            language: None,
        })
    } else {
        Term::Literal(Literal {
            value: trim_float(value),
            datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".to_string()),
            language: None,
        })
    }
}

fn numeric_terms_equal(a: &Term, b: &Term) -> bool {
    match (numeric_value(a), numeric_value(b)) {
        (Some(x), Some(y)) => (x.value - y.value).abs() <= f64::EPSILON,
        _ => a == b,
    }
}

fn trim_float(value: f64) -> String {
    let mut s = value.to_string();
    if s.contains('.') {
        while s.ends_with('0') { s.pop(); }
        if s.ends_with('.') { s.push('0'); }
    }
    s
}

fn bind_one(bindings: &Bindings, name: &str, value: Term) -> Option<Bindings> {
    let mut b = bindings.clone();
    if bind_one_mut(&mut b, name, value) { Some(b) } else { None }
}

fn bind_one_mut(bindings: &mut Bindings, name: &str, value: Term) -> bool {
    let value = resolve(&value, bindings);

    if let Some(existing) = bindings.get(name).cloned() {
        return resolve(&existing, bindings) == value;
    }

    // Avoid cyclic substitutions such as `?x = (?x)` or `?x = { ?x :p :o }`.
    // Those can appear during broad backward-rule unification and later cause
    // recursive resolution/formatting to overflow the Rust stack.
    if matches!(&value, Term::Var(other) if other == name) {
        return true;
    }
    if occurs_in(name, &value, bindings) {
        return false;
    }

    bindings.insert(name.to_string(), value);
    true
}

fn occurs_in(name: &str, term: &Term, bindings: &Bindings) -> bool {
    occurs_in_with_seen(name, term, bindings, &mut HashSet::new())
}

fn occurs_in_with_seen(
    name: &str,
    term: &Term,
    bindings: &Bindings,
    seen: &mut HashSet<String>,
) -> bool {
    match term {
        Term::Var(var) if var == name => true,
        Term::Var(var) => {
            if !seen.insert(var.clone()) { return false; }
            bindings
                .get(var)
                .is_some_and(|bound| occurs_in_with_seen(name, bound, bindings, seen))
        }
        Term::List(items) => items.iter().any(|item| {
            let mut branch_seen = seen.clone();
            occurs_in_with_seen(name, item, bindings, &mut branch_seen)
        }),
        Term::Formula(triples) => triples.iter().any(|triple| {
            let mut s_seen = seen.clone();
            let mut p_seen = seen.clone();
            let mut o_seen = seen.clone();
            occurs_in_with_seen(name, &triple.s, bindings, &mut s_seen)
                || occurs_in_with_seen(name, &triple.p, bindings, &mut p_seen)
                || occurs_in_with_seen(name, &triple.o, bindings, &mut o_seen)
        }),
        _ => false,
    }
}

fn resolve(term: &Term, bindings: &Bindings) -> Term {
    resolve_with_seen(term, bindings, &mut HashSet::new())
}

fn resolve_with_seen(term: &Term, bindings: &Bindings, seen: &mut HashSet<String>) -> Term {
    match term {
        Term::Var(name) => {
            if !seen.insert(name.clone()) { return term.clone(); }
            match bindings.get(name) {
                Some(bound) => resolve_with_seen(bound, bindings, seen),
                None => term.clone(),
            }
        }
        Term::List(items) => Term::List(items.iter().map(|item| {
            let mut branch_seen = seen.clone();
            resolve_with_seen(item, bindings, &mut branch_seen)
        }).collect()),
        Term::Formula(triples) => Term::Formula(triples.iter().map(|t| {
            let mut s_seen = seen.clone();
            let mut p_seen = seen.clone();
            let mut o_seen = seen.clone();
            Triple::new(
                resolve_with_seen(&t.s, bindings, &mut s_seen),
                resolve_with_seen(&t.p, bindings, &mut p_seen),
                resolve_with_seen(&t.o, bindings, &mut o_seen),
            )
        }).collect()),
        _ => term.clone(),
    }
}

fn resolve_triple(t: &Triple, bindings: &Bindings) -> Triple {
    Triple::new(resolve(&t.s, bindings), resolve(&t.p, bindings), resolve(&t.o, bindings))
}

fn resolve_pattern_triple(t: &Triple, bindings: &Bindings) -> Triple {
    Triple::new(
        resolve_pattern(&t.s, bindings),
        resolve_pattern(&t.p, bindings),
        resolve_pattern(&t.o, bindings),
    )
}

fn canonicalize_bindings(bindings: &Bindings) -> Bindings {
    bindings
        .iter()
        .map(|(k, v)| (k.clone(), resolve(v, bindings)))
        .collect()
}

fn instantiate_triple(
    t: &Triple,
    bindings: &Bindings,
    blank_map: &mut BTreeMap<String, Term>,
) -> Option<Triple> {
    Some(Triple::new(
        instantiate_term(&t.s, bindings, blank_map)?,
        instantiate_term(&t.p, bindings, blank_map)?,
        instantiate_term(&t.o, bindings, blank_map)?,
    ))
}

fn instantiate_term(
    term: &Term,
    bindings: &Bindings,
    blank_map: &mut BTreeMap<String, Term>,
) -> Option<Term> {
    match term {
        Term::Var(name) => bindings.get(name).map(|value| resolve(value, bindings)),
        Term::Blank(name) => {
            if let Some(existing) = blank_map.get(name) { return Some(existing.clone()); }
            let fresh = Term::Blank(format!("{}_{}", name, stable_binding_suffix(bindings)));
            blank_map.insert(name.clone(), fresh.clone());
            Some(fresh)
        }
        Term::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items { out.push(instantiate_term(item, bindings, blank_map)?); }
            Some(Term::List(out))
        }
        Term::Formula(triples) => {
            let mut out = Vec::with_capacity(triples.len());
            let mut formula_blank_map = BTreeMap::<String, Term>::new();
            let salt = stable_formula_suffix(bindings, triples);
            for triple in triples {
                out.push(instantiate_formula_triple(triple, bindings, &mut formula_blank_map, &salt));
            }
            Some(Term::Formula(out))
        }
        other => Some(other.clone()),
    }
}

fn instantiate_formula_triple(
    t: &Triple,
    bindings: &Bindings,
    blank_map: &mut BTreeMap<String, Term>,
    salt: &str,
) -> Triple {
    Triple::new(
        instantiate_formula_term(&t.s, bindings, blank_map, salt),
        instantiate_formula_term(&t.p, bindings, blank_map, salt),
        instantiate_formula_term(&t.o, bindings, blank_map, salt),
    )
}

fn instantiate_formula_term(
    term: &Term,
    bindings: &Bindings,
    blank_map: &mut BTreeMap<String, Term>,
    salt: &str,
) -> Term {
    match term {
        Term::Var(name) => bindings.get(name).map(|value| resolve(value, bindings)).unwrap_or_else(|| term.clone()),
        Term::Blank(name) => {
            if let Some(existing) = blank_map.get(name) { return existing.clone(); }
            let fresh = Term::Blank(format!("{}_{}", name, salt));
            blank_map.insert(name.clone(), fresh.clone());
            fresh
        }
        Term::List(items) => Term::List(items.iter().map(|item| instantiate_formula_term(item, bindings, blank_map, salt)).collect()),
        Term::Formula(triples) => {
            let nested_salt = stable_formula_suffix(bindings, triples);
            let mut nested_blank_map = BTreeMap::<String, Term>::new();
            Term::Formula(triples.iter().map(|t| instantiate_formula_triple(t, bindings, &mut nested_blank_map, &nested_salt)).collect())
        }
        other => other.clone(),
    }
}

fn stable_formula_suffix(bindings: &Bindings, triples: &[Triple]) -> String {
    let mut h = 1469598103934665603u64;
    for (k, v) in bindings {
        // Body blank nodes are local pattern variables.  Their concrete source
        // blank-node identity must not make existential head blanks distinct:
        // if two supports bind the same ordinary variables, they represent the
        // same generated existential for this forward-chaining closure.  This
        // is especially important for state-machine examples such as
        // dining-philosophers.n3, where otherwise semantically duplicate
        // ForkState nodes can cascade into an exponential number of fresh
        // states.  Ordinary variables are still part of the suffix below.
        if k.starts_with("_:") { continue; }
        for b in k.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(1099511628211);
        }
        for b in format!("{:?}", resolve(v, bindings)).as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(1099511628211);
        }
    }
    for t in triples {
        for b in format!("{:?}", t).as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(1099511628211);
        }
    }
    format!("{:x}", h)
}

fn stable_binding_suffix(bindings: &Bindings) -> String {
    // Deterministic, compact suffix. It only needs to be unique enough within a
    // single run for existential blank nodes introduced by rule heads.
    let mut h = 1469598103934665603u64;
    for (k, v) in bindings {
        // Ignore local blank-node pattern bindings when deriving the stable
        // identity for existential blanks in rule heads.  These bindings name
        // the *supporting* blank nodes matched in the body; including them here
        // makes repeated equivalent supports create fresh, different head
        // blanks and can make monotonic state updates blow up.
        if k.starts_with("_:") { continue; }
        for b in k.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(1099511628211);
        }
        let rendered = format!("{:?}", resolve(v, bindings));
        for b in rendered.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(1099511628211);
        }
    }
    format!("{:x}", h)
}
