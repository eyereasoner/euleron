use eyeron::incremental::{blank, iri};
use eyeron::{parse_n3, IncrementalReasoner, Quad, RdfTerm};

const EX: &str = "http://example.org/";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// Rules are still authored in N3 and parsed by eyeron's own parser.
fn rules_from(source: &str) -> Vec<eyeron::Rule> {
    parse_n3(source, None).unwrap().rules
}

fn ex(local: &str) -> RdfTerm { iri(format!("{EX}{local}")).unwrap() }
fn rdfs(local: &str) -> RdfTerm { iri(format!("{RDFS}{local}")).unwrap() }
fn rdf(local: &str) -> RdfTerm { iri(format!("{RDF}{local}")).unwrap() }

/// A quad in the default graph. Facts are never parsed from N3 -- they are
/// plain quads, exactly as they'd arrive from a real quad store/parser.
fn quad(s: RdfTerm, p: RdfTerm, o: RdfTerm) -> Quad {
    Quad::new(s, p, o, None)
}

const SUBPROPERTY_TRANSITIVITY: &str = r#"
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
    { ?a rdfs:subPropertyOf ?b . ?b rdfs:subPropertyOf ?c . } => { ?a rdfs:subPropertyOf ?c . } .
"#;

const SUBPROPERTY_TO_TRIPLE: &str = r#"
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
    { ?x ?p ?y . ?p rdfs:subPropertyOf ?q . } => { ?x ?q ?y . } .
"#;

const SUBCLASS_RULES: &str = r#"
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
    { ?a rdfs:subClassOf ?b . ?b rdfs:subClassOf ?c . } => { ?a rdfs:subClassOf ?c . } .
    { ?x a ?c . ?c rdfs:subClassOf ?d . } => { ?x a ?d . } .
"#;

#[test]
fn rdfs_sub_property_of_propagates_triples_along_the_hierarchy() {
    // Adding rdfs:subPropertyOf between two properties should make existing
    // (and future) triples on the narrower property visible on the broader
    // one, and removing it should take those derived triples back out.
    let rules = rules_from(SUBPROPERTY_TO_TRIPLE);
    let mut r = IncrementalReasoner::new(rules).unwrap();

    r.assert_fact(quad(ex("alice"), ex("hasBiologicalParent"), ex("bob")));

    let bridge = quad(ex("hasBiologicalParent"), rdfs("subPropertyOf"), ex("hasParent"));
    let delta = r.assert_fact(bridge.clone());

    let derived = quad(ex("alice"), ex("hasParent"), ex("bob"));
    assert!(delta.added.contains(&derived), "expected {derived:?} to be newly derived, got {delta:?}");
    assert!(r.contains(&derived));

    let delta = r.retract_fact(&bridge);
    assert!(delta.removed.contains(&derived), "expected {derived:?} to be retracted, got {delta:?}");
    assert!(!r.contains(&derived));
}

#[test]
fn rdfs_sub_property_of_is_transitive_and_incremental() {
    // hasBiologicalParent < hasParent < hasRelative, built up one triple at a
    // time; each addition should only report what's newly true.
    let rules = rules_from(SUBPROPERTY_TRANSITIVITY);
    let mut r = IncrementalReasoner::new(rules).unwrap();

    let bp_parent = quad(ex("hasBiologicalParent"), rdfs("subPropertyOf"), ex("hasParent"));
    let parent_rel = quad(ex("hasParent"), rdfs("subPropertyOf"), ex("hasRelative"));
    let bp_rel = quad(ex("hasBiologicalParent"), rdfs("subPropertyOf"), ex("hasRelative"));

    let delta = r.assert_fact(bp_parent.clone());
    assert_eq!(delta.added, vec![bp_parent.clone()]);

    let delta = r.assert_fact(parent_rel.clone());
    assert!(delta.added.contains(&parent_rel));
    assert!(delta.added.contains(&bp_rel), "transitive closure should appear in the same delta: {delta:?}");
    assert!(r.contains(&bp_rel));

    // Breaking the middle link should remove the transitive consequence too.
    let delta = r.retract_fact(&parent_rel);
    assert!(delta.removed.contains(&bp_rel));
    assert!(delta.removed.contains(&parent_rel));
    assert!(!r.contains(&bp_rel));
    assert!(r.contains(&bp_parent), "unrelated link must survive");
}

#[test]
fn rdfs_sub_class_of_reclassifies_instances_on_add_and_remove() {
    let rules = rules_from(SUBCLASS_RULES);
    let mut r = IncrementalReasoner::new(rules).unwrap();

    r.assert_fact(quad(ex("Rex"), rdf("type"), ex("Dog")));
    let link = quad(ex("Dog"), rdfs("subClassOf"), ex("Mammal"));
    let delta = r.assert_fact(link.clone());

    let derived = quad(ex("Rex"), rdf("type"), ex("Mammal"));
    assert!(delta.added.contains(&derived));

    let delta = r.retract_fact(&link);
    assert!(delta.removed.contains(&derived));
    assert!(!r.contains(&derived));
}

#[test]
fn a_conclusion_with_two_independent_derivations_survives_removing_one() {
    // :Rex is a :Mammal both via :Dog subClassOf :Mammal and directly via
    // :Canine subClassOf :Mammal (:Rex is also asserted a :Canine). Removing
    // one subClassOf link must not remove the shared conclusion.
    let rules = rules_from(SUBCLASS_RULES);
    let mut r = IncrementalReasoner::new(rules).unwrap();

    r.assert_fact(quad(ex("Rex"), rdf("type"), ex("Dog")));
    r.assert_fact(quad(ex("Rex"), rdf("type"), ex("Canine")));
    let dog_mammal = quad(ex("Dog"), rdfs("subClassOf"), ex("Mammal"));
    let canine_mammal = quad(ex("Canine"), rdfs("subClassOf"), ex("Mammal"));
    r.assert_fact(dog_mammal.clone());
    r.assert_fact(canine_mammal.clone());

    let derived = quad(ex("Rex"), rdf("type"), ex("Mammal"));
    assert!(r.contains(&derived));

    let delta = r.retract_fact(&dog_mammal);
    assert!(!delta.removed.contains(&derived), "still derivable via :Canine: {delta:?}");
    assert!(r.contains(&derived));

    let delta = r.retract_fact(&canine_mammal);
    assert!(delta.removed.contains(&derived), "last support gone, must be retracted: {delta:?}");
    assert!(!r.contains(&derived));
}

#[test]
fn retracting_a_fact_that_was_never_explicit_is_a_no_op() {
    let rules = rules_from(SUBCLASS_RULES);
    let mut r = IncrementalReasoner::new(rules).unwrap();
    r.assert_fact(quad(ex("Rex"), rdf("type"), ex("Dog")));
    r.assert_fact(quad(ex("Dog"), rdfs("subClassOf"), ex("Mammal")));

    let derived = quad(ex("Rex"), rdf("type"), ex("Mammal"));
    assert!(r.contains(&derived));

    // The derived fact was never asserted explicitly, so retracting it
    // directly must not do anything (its support comes entirely from the
    // rule firing, not from an explicit assertion).
    let delta = r.retract_fact(&derived);
    assert!(delta.removed.is_empty());
    assert!(r.contains(&derived));
}

#[test]
fn reasoning_only_joins_premises_from_the_same_graph() {
    // Confirms the "same graph per rule firing" semantics: a rule firing
    // must not join facts from two different named graphs, and its
    // conclusion lands in that same graph.
    let rules = rules_from(SUBPROPERTY_TRANSITIVITY);
    let mut r = IncrementalReasoner::new(rules).unwrap();

    let graph_a = Some(ex("GraphA"));
    let graph_b = Some(ex("GraphB"));

    r.assert_fact(Quad::new(ex("p"), rdfs("subPropertyOf"), ex("q"), graph_a.clone()));
    let delta = r.assert_fact(Quad::new(ex("q"), rdfs("subPropertyOf"), ex("r"), graph_a.clone()));

    let pr_a = Quad::new(ex("p"), rdfs("subPropertyOf"), ex("r"), graph_a.clone());
    assert!(delta.added.contains(&pr_a), "transitivity should fire within graph A: {delta:?}");
    assert!(r.contains(&pr_a));

    // The matching half of the chain in graph B alone must not let the rule
    // fire there -- graph A's p-subPropertyOf-q fact does not apply to it.
    let delta = r.assert_fact(Quad::new(ex("q"), rdfs("subPropertyOf"), ex("r"), graph_b.clone()));
    let pr_b = Quad::new(ex("p"), rdfs("subPropertyOf"), ex("r"), graph_b.clone());
    assert!(!delta.added.contains(&pr_b));
    assert!(!r.contains(&pr_b));
}

fn list_member_and_union_of_rules() -> Vec<eyeron::Rule> {
    // { ?l rdf:first ?m . } => { ?l :listMember ?m . } .
    // { ?l rdf:rest ?more . ?more :listMember ?m . } => { ?l :listMember ?m . } .
    // { ?c :unionOf ?l . ?l :listMember ?m . ?x a ?m . } => { ?x a ?c . } .
    rules_from(r#"
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix : <http://example.org/> .
        { ?l rdf:first ?m . } => { ?l :listMember ?m . } .
        { ?l rdf:rest ?more . ?more :listMember ?m . } => { ?l :listMember ?m . } .
        { ?c :unionOf ?l . ?l :listMember ?m . ?x a ?m . } => { ?x a ?c . } .
    "#)
}

#[test]
fn rdf_first_rest_support_owl_union_of_style_reasoning() {
    // Real RDF has no first-class "list" term -- (:Dog :Cat) is always a
    // blank-node chain of rdf:first/rdf:rest/rdf:nil triples, built here by
    // hand exactly as a real RDF parser would produce it. rdf:first/rdf:rest
    // need no special evaluation: they are ordinary matchable predicates.
    let mut r = IncrementalReasoner::new(list_member_and_union_of_rules()).unwrap();

    let head = blank("l1").unwrap();
    let tail = blank("l2").unwrap();
    let union = quad(ex("Pet"), ex("unionOf"), head.clone());
    r.assert_fact(union.clone());
    r.assert_fact(quad(head.clone(), rdf("first"), ex("Dog")));
    r.assert_fact(quad(head, rdf("rest"), tail.clone()));
    r.assert_fact(quad(tail.clone(), rdf("first"), ex("Cat")));
    r.assert_fact(quad(tail, rdf("rest"), rdf("nil")));

    r.assert_fact(quad(ex("Rex"), rdf("type"), ex("Dog")));
    let delta = r.assert_fact(quad(ex("Whiskers"), rdf("type"), ex("Cat")));

    assert!(r.contains(&quad(ex("Rex"), rdf("type"), ex("Pet"))));
    assert!(delta.added.contains(&quad(ex("Whiskers"), rdf("type"), ex("Pet"))));

    // Retracting the union-of fact must cascade through the whole
    // rdf:first/rdf:rest chain and both memberships.
    let delta = r.retract_fact(&union);
    assert!(delta.removed.contains(&quad(ex("Rex"), rdf("type"), ex("Pet"))));
    assert!(delta.removed.contains(&quad(ex("Whiskers"), rdf("type"), ex("Pet"))));
    assert!(!r.contains(&quad(ex("Rex"), rdf("type"), ex("Pet"))));
}

#[test]
fn backward_rules_are_rejected_up_front() {
    let doc = parse_n3(
        r#"@prefix : <http://example.org/> . { ?x :q ?y . } <= { ?x :p ?y . } ."#,
        None,
    )
    .unwrap();
    assert!(IncrementalReasoner::new(doc.rules).is_err());
}

#[test]
fn non_list_builtin_predicates_are_rejected_up_front() {
    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            @prefix math: <http://www.w3.org/2000/10/swap/math#> .
            { (?a 1) math:sum ?b . } => { :x :y ?b . } .
        "#,
        None,
    )
    .unwrap();
    assert!(IncrementalReasoner::new(doc.rules).is_err());
}
