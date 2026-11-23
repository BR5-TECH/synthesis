//! `validate_surface` (AUI-FR-EGAB to AUI-FR-VOYL).

use serde_json::{json, Value};

use crate::agent_surfaces::validate::{validate_surface, MAX_BYTES, MAX_COMPONENTS, MAX_DEPTH};

/// A valid form: a choice, a note, and a Send button.
pub(crate) fn form() -> Vec<Value> {
    serde_json::from_value(json!([
        {"id":"root","component":"Column","children":["q","pick","note","send"]},
        {"id":"q","component":"Text","text":"Which storage do you want?"},
        {"id":"pick","component":"ChoicePicker","options":[{"label":"SQLite","value":"sqlite"},{"label":"Files","value":"files"}],"value":{"@path":"/choice"}},
        {"id":"note","component":"TextField","label":"Note","value":{"@path":"/note"}},
        {"id":"send","component":"Button","variant":"primary","child":"send_label",
         "action":{"event":{"name":"choose_storage","context":{"choice":{"@path":"/choice"},"note":{"@path":"/note"},"kind":"storage"}}},
         "@checks":[{"condition":{"@call":"required","args":{"value":{"@path":"/choice"}}},"message":"Pick one."}]},
        {"id":"send_label","component":"Text","text":"Send"}
    ]))
    .unwrap()
}

pub(crate) fn model() -> Value {
    json!({"choice": [], "note": ""})
}

fn rule_of(components: &[Value], data_model: &Value) -> &'static str {
    validate_surface(components, data_model).expect_err("a violation").rule
}

/// `form()` with one component changed.
fn with(id: &str, change: impl Fn(&mut Value)) -> Vec<Value> {
    let mut components = form();
    let target = components.iter_mut().find(|c| c["id"] == id).expect("component");
    change(target);
    components
}

// AUI-FR-EGAB: the instruction example and a form are valid.
#[test]
fn a_well_formed_surface_is_valid() {
    validate_surface(&form(), &model()).expect("valid");
}

// AUI-FR-PYVB / AUI-FR-EGAB: the example in the catalog instructions is valid.
#[test]
fn the_example_of_the_instructions_is_valid() {
    let instructions = crate::agent_surfaces::catalog().instructions();
    let start = instructions.find("Example: ").expect("the example") + "Example: ".len();
    let components: Vec<Value> = serde_json::from_str(&instructions[start..]).expect("JSON");
    validate_surface(&components, &json!({})).expect("valid");
}

// AUI-FR-EGAB / AUI-FR-PMXJ: every catalog component in one valid surface.
#[test]
fn every_component_of_the_catalog_can_be_used() {
    let components: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Tabs","tabs":[{"title":"One","child":"col"},{"title":"Two","child":"card"}]},
        {"id":"col","component":"Column","justify":"start","align":"stretch","children":["row","list","table"]},
        {"id":"row","component":"Row","children":["icon","div","slider"]},
        {"id":"icon","component":"Icon","name":"info"},
        {"id":"div","component":"Divider","axis":"vertical"},
        {"id":"slider","component":"Slider","max":10,"min":0,"steps":1,"value":{"@path":"/n"},"label":"N"},
        {"id":"list","component":"List","direction":"vertical","children":{"componentId":"item","path":"/items"}},
        {"id":"item","component":"Text","text":{"@path":"name"},"variant":"caption"},
        {"id":"table","component":"Table","columns":[{"key":"a","title":"A"}],"rows":[{"a":"1"}],"caption":"T"},
        {"id":"card","component":"Card","child":"inner"},
        {"id":"inner","component":"Column","children":["check","date","btn"]},
        {"id":"check","component":"CheckBox","label":"Ok","value":{"@path":"/ok"}},
        {"id":"date","component":"DateTimeInput","value":{"@path":"/when"},"enableDate":true},
        {"id":"btn","component":"Button","child":"lbl","action":{"event":{"name":"go"}}},
        {"id":"lbl","component":"Text","text":{"@call":"formatString","args":{"value":"Go ${/n}"}}}
    ]))
    .unwrap();
    validate_surface(&components, &json!({"n": 1, "items": [], "ok": false, "when": ""}))
        .expect("valid");
}

// AUI-FR-EGAB: an unknown component, a missing required property, and an
// undeclared property are each refused with the component and the property.
#[test]
fn a_component_the_catalog_does_not_declare_is_refused() {
    for kind in ["Modal", "Image", "Video", "AudioPlayer", "Html"] {
        let components = with("q", |c| c["component"] = json!(kind));
        let violation = validate_surface(&components, &model()).unwrap_err();
        assert_eq!(violation.rule, "unknown_component", "{kind}");
        assert_eq!(violation.component_id.as_deref(), Some("q"));
        assert_eq!(violation.property.as_deref(), Some("component"));
    }
    let violation = validate_surface(
        &with("q", |c| c.as_object_mut().unwrap().remove("text").map(|_| ()).unwrap()),
        &model(),
    )
    .unwrap_err();
    assert_eq!((violation.rule, violation.property.as_deref()), ("missing_property", Some("text")));
    let violation =
        validate_surface(&with("q", |c| c["html"] = json!("<b>x</b>")), &model()).unwrap_err();
    assert_eq!((violation.rule, violation.property.as_deref()), ("unknown_property", Some("html")));
    let violation =
        validate_surface(&with("q", |c| c["variant"] = json!("h1")), &model()).unwrap_err();
    assert_eq!((violation.rule, violation.property.as_deref()), ("invalid_value", Some("variant")));
    assert!(violation.expected.contains("caption"));
}

// AUI-FR-GMJK: one `root`, unique ids, and ids of the allowed form and length.
#[test]
fn the_ids_of_a_surface_are_checked() {
    assert_eq!(rule_of(&with("root", |c| c["id"] = json!("top")), &model()), "no_root");
    let mut twice = form();
    twice.push(json!({"id":"q","component":"Text","text":"again"}));
    assert_eq!(rule_of(&twice, &model()), "duplicate_id");
    let mut many_roots = form();
    many_roots.push(json!({"id":"root","component":"Text","text":"again"}));
    assert_eq!(rule_of(&many_roots, &model()), "duplicate_id");
    for bad in ["", "has space", "a/b", &"x".repeat(65)] {
        let components = with("q", |c| c["id"] = json!(bad));
        assert_eq!(rule_of(&components, &model()), "bad_id", "{bad:?}");
    }
    // An id of exactly 64 characters is valid.
    let long = "y".repeat(64);
    let mut components = with("q", |c| c["id"] = json!(long));
    components[0]["children"][0] = json!(long);
    validate_surface(&components, &model()).expect("64 characters are valid");
}

// AUI-FR-AQBE: every reference resolves, every component is reachable, and
// no component contains itself.
#[test]
fn the_tree_of_a_surface_is_checked() {
    let missing = with("root", |c| c["children"] = json!(["q", "pick", "note", "send", "ghost"]));
    let violation = validate_surface(&missing, &model()).unwrap_err();
    assert_eq!(
        (violation.rule, violation.component_id.as_deref()),
        ("missing_child", Some("root"))
    );
    let mut orphan = form();
    orphan.push(json!({"id":"lost","component":"Text","text":"nobody"}));
    let violation = validate_surface(&orphan, &model()).unwrap_err();
    assert_eq!((violation.rule, violation.component_id.as_deref()), ("unreachable", Some("lost")));
    let cycle: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Column","children":["a"]},
        {"id":"a","component":"Card","child":"b"},
        {"id":"b","component":"Card","child":"a"}
    ]))
    .unwrap();
    assert_eq!(rule_of(&cycle, &json!({})), "cycle");
    let self_loop: Vec<Value> =
        serde_json::from_value(json!([{"id":"root","component":"Card","child":"root"}])).unwrap();
    assert_eq!(rule_of(&self_loop, &json!({})), "cycle");
}

/// A chain of `depth` components from `root`.
fn chain(depth: usize) -> Vec<Value> {
    let mut components = Vec::new();
    for level in 0..depth {
        let id = if level == 0 { "root".to_string() } else { format!("c{level}") };
        if level + 1 == depth {
            components.push(json!({"id": id, "component": "Text", "text": "leaf"}));
        } else {
            components
                .push(json!({"id": id, "component": "Card", "child": format!("c{}", level + 1)}));
        }
    }
    components
}

// AUI-FR-ZEXT: at most 100 components and a depth of at most 8.
#[test]
fn the_size_and_the_depth_of_a_surface_are_bounded() {
    validate_surface(&chain(MAX_DEPTH), &json!({})).expect("depth 8 is valid");
    assert_eq!(rule_of(&chain(MAX_DEPTH + 1), &json!({})), "too_deep");
    let mut wide = vec![
        json!({"id":"root","component":"Column","children":(1..MAX_COMPONENTS).map(|i| format!("t{i}")).collect::<Vec<_>>()}),
    ];
    wide.extend(
        (1..MAX_COMPONENTS).map(|i| json!({"id": format!("t{i}"), "component":"Text","text":"x"})),
    );
    assert_eq!(wide.len(), MAX_COMPONENTS);
    validate_surface(&wide, &json!({})).expect("100 components are valid");
    wide.push(json!({"id":"extra","component":"Text","text":"x"}));
    assert_eq!(rule_of(&wide, &json!({})), "too_many_components");
    assert_eq!(rule_of(&[], &json!({})), "no_components");
}

// AUI-FR-IAGR: 32 KiB of JSON at most, and a data model that is an object.
#[test]
fn the_bytes_and_the_data_model_of_a_surface_are_checked() {
    let components: Vec<Value> =
        serde_json::from_value(json!([{"id":"root","component":"Text","text":"x"}])).unwrap();
    let fits = "a".repeat(MAX_BYTES - 200);
    validate_surface(&components, &json!({ "big": fits })).expect("under the bound");
    let too_big = "a".repeat(MAX_BYTES);
    assert_eq!(rule_of(&components, &json!({ "big": too_big })), "too_large");
    for not_object in [json!([]), json!("x"), json!(null), json!(3)] {
        assert_eq!(rule_of(&components, &not_object), "data_model_not_object");
    }
}

// AUI-FR-HCCU: a call names a catalog function with that function's arguments.
#[test]
fn a_call_must_name_a_catalog_function_with_its_arguments() {
    let call = |call: Value| with("q", move |c| c["text"] = call.clone());
    assert_eq!(
        rule_of(&call(json!({"@call":"openUrl","args":{"url":"https://x"}})), &model()),
        "unknown_function"
    );
    assert_eq!(
        rule_of(&call(json!({"@call":"regex","args":{"value":"a","pattern":"a"}})), &model()),
        "unknown_function"
    );
    let violation =
        validate_surface(&call(json!({"@call":"formatString","args":{"text":"a"}})), &model())
            .unwrap_err();
    assert_eq!(violation.rule, "bad_call");
    assert_eq!(violation.component_id.as_deref(), Some("q"));
    validate_surface(&call(json!({"@call":"pluralize","args":{"value":{"@path":"/n"},"other":"items","one":"item"}})), &model()).expect("valid");
    // A call nested in a call is checked as well.
    let nested = json!({"@call":"not","args":{"value":{"@call":"openUrl","args":{}}}});
    let components = with("send", move |c| c["@checks"][0]["condition"] = nested.clone());
    assert_eq!(rule_of(&components, &model()), "unknown_function");
}

// AUI-FR-HCCU: a binding is `{"@path": "<pointer>"}` and nothing else.
#[test]
fn a_binding_has_one_path_and_nothing_else() {
    let bind = |value: Value| with("note", move |c| c["value"] = value.clone());
    assert_eq!(rule_of(&bind(json!({"@path": 3})), &model()), "invalid_value");
    let rows = |rows: Value| {
        let mut components = form();
        components[0]["children"] = json!(["q", "pick", "note", "send", "t"]);
        components.push(
            json!({"id":"t","component":"Table","columns":[{"key":"a","title":"A"}],"rows":rows}),
        );
        components
    };
    // A binding with another key is refused where it is meant as a binding,
    // and where it stands inside a literal.
    assert_eq!(rule_of(&rows(json!({"@path":"/rows","extra":1})), &model()), "unknown_property");
    assert_eq!(rule_of(&rows(json!([{"a":{"@path":"/x","b":1}}])), &model()), "bad_binding");
    validate_surface(&rows(json!({"@path":"/rows"})), &model()).expect("a bound table");
}

// AUI-FR-LXOY: a JSON Pointer, relative only inside a template, and
// `@index` only there too.
#[test]
fn a_pointer_is_checked_against_where_it_stands() {
    let bind = |pointer: &str| {
        let pointer = pointer.to_string();
        with("note", move |c| c["value"] = json!({"@path": pointer}))
    };
    assert_eq!(rule_of(&bind("note"), &model()), "relative_path_outside_template");
    assert_eq!(rule_of(&bind("/a~2b"), &model()), "bad_pointer");
    validate_surface(&bind("/a~0b/c~1d"), &model()).expect("escapes are valid");
    let index = with("q", |c| c["text"] = json!({"@call":"@index","args":{}}));
    assert_eq!(rule_of(&index, &model()), "index_outside_template");
    let template: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"List","children":{"componentId":"row","path":"/items"}},
        {"id":"row","component":"Row","children":["name","pos"]},
        {"id":"name","component":"Text","text":{"@path":"name"}},
        {"id":"pos","component":"Text","text":{"@call":"formatNumber","args":{"value":{"@call":"@index","args":{}}}}}
    ]))
    .unwrap();
    validate_surface(&template, &json!({"items": []})).expect("relative paths inside a template");
}

// AUI-FR-JLHW: an action has an event whose name has the allowed form, and
// whose context holds literals and bindings alone.
#[test]
fn an_action_is_checked() {
    let named = |name: &str| {
        let name = name.to_string();
        with("send", move |c| c["action"]["event"]["name"] = json!(name))
    };
    for bad in ["Choose", "1go", "go-now", "", &"a".repeat(65)] {
        let violation = validate_surface(&named(bad), &model()).unwrap_err();
        assert_eq!(violation.property.as_deref(), Some("action.event.name"), "{bad:?}");
    }
    validate_surface(&named(&"a".repeat(64)), &model()).expect("64 characters are valid");
    let no_event = with("send", |c| c["action"] = json!({}));
    assert_eq!(rule_of(&no_event, &model()), "missing_property");
    let call = with("send", |c| {
        c["action"]["event"]["context"]["x"] = json!({"@call":"not","args":{"value":true}})
    });
    assert_eq!(rule_of(&call, &model()), "call_in_context");
    let function_call = with("send", |c| {
        c["action"]["functionCall"] = json!({"@call":"not","args":{"value":true}})
    });
    assert_eq!(rule_of(&function_call, &model()), "unknown_property");
}

// AUI-FR-MTMS: an input on a surface with no Button is not valid.
#[test]
fn inputs_need_a_button() {
    let inputs: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Column","children":["note"]},
        {"id":"note","component":"TextField","label":"Note","value":{"@path":"/note"}}
    ]))
    .unwrap();
    assert_eq!(rule_of(&inputs, &json!({"note": ""})), "inputs_without_button");
    let display: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Table","columns":[{"key":"a","title":"A"}],"rows":[{"a":1}]}
    ]))
    .unwrap();
    validate_surface(&display, &json!({})).expect("a display surface needs no button");
}

// AUI-FR-VOYL: the violation names the rule, the component, the property,
// and what a valid value looks like.
#[test]
fn a_violation_names_where_it_is() {
    let violation =
        validate_surface(&with("pick", |c| c["variant"] = json!("many")), &model()).unwrap_err();
    assert_eq!(violation.rule, "invalid_value");
    assert_eq!(violation.component_id.as_deref(), Some("pick"));
    assert_eq!(violation.property.as_deref(), Some("variant"));
    assert!(violation.expected.contains("multipleSelection"));
    let violation =
        validate_surface(&with("pick", |c| c["options"][1]["value"] = json!(2)), &model())
            .unwrap_err();
    assert_eq!(violation.property.as_deref(), Some("options.1.value"));
}

// AUI contract surface: `length` and `numeric` take a bound, `weight` stands
// in a Row or a Column alone, and literal Table rows are objects.
#[test]
fn the_contract_rules_beyond_the_schema_are_checked() {
    for call in [
        json!({"@call":"length","args":{"value":"x"}}),
        json!({"@call":"numeric","args":{"value":1}}),
    ] {
        let components = with("send", move |c| c["@checks"][0]["condition"] = call.clone());
        assert_eq!(rule_of(&components, &model()), "bad_call");
    }
    let bounded = json!({"@call":"length","args":{"value":{"@path":"/note"},"max":10}});
    validate_surface(
        &with("send", move |c| c["@checks"][0]["condition"] = bounded.clone()),
        &model(),
    )
    .expect("a bound call is valid");
    // `root` of `form()` is a Column, so its children take a weight.
    validate_surface(&with("q", |c| c["weight"] = json!(1)), &model()).expect("in a Column");
    assert_eq!(
        rule_of(&with("root", |c| c["weight"] = json!(1)), &model()),
        "weight_outside_row_or_column"
    );
    assert_eq!(
        rule_of(&with("send_label", |c| c["weight"] = json!(1)), &model()),
        "weight_outside_row_or_column"
    );
    let table = |rows: Value| -> Vec<Value> {
        serde_json::from_value(json!([{"id":"root","component":"Table","columns":[{"key":"a","title":"A"}],"rows":rows}])).unwrap()
    };
    for bad in [json!("abc"), json!(3), json!([1, 2]), json!({"a": 1})] {
        let violation = validate_surface(&table(bad.clone()), &json!({})).unwrap_err();
        assert_eq!(
            (violation.rule, violation.property.as_deref()),
            ("invalid_value", Some("rows")),
            "{bad}"
        );
    }
    validate_surface(&table(json!([])), &json!({})).expect("no rows");
    validate_surface(&table(json!({"@path":"/rows"})), &json!({})).expect("bound rows");
}

// AUI-FR-AQBE: an unreachable cycle is unreachable, a component with two
// parents is valid, and depth counts through every kind of child link.
#[test]
fn reach_and_depth_count_every_link() {
    let lost_cycle: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Text","text":"x"},
        {"id":"a","component":"Card","child":"b"},
        {"id":"b","component":"Card","child":"a"}
    ]))
    .unwrap();
    assert_eq!(rule_of(&lost_cycle, &json!({})), "unreachable");
    let diamond: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Column","children":["a","b"]},
        {"id":"a","component":"Card","child":"leaf"},
        {"id":"b","component":"Card","child":"leaf"},
        {"id":"leaf","component":"Text","text":"x"}
    ]))
    .unwrap();
    validate_surface(&diamond, &json!({})).expect("two parents are valid");
    // Tabs, a Button, and a template each count one level.
    let deep = |extra: usize| -> Vec<Value> {
        let mut c: Vec<Value> = serde_json::from_value(json!([
            {"id":"root","component":"Tabs","tabs":[{"title":"T","child":"btn"}]},
            {"id":"btn","component":"Button","child":"list","action":{"event":{"name":"go"}}},
            {"id":"list","component":"List","children":{"componentId":"c0","path":"/items"}}
        ]))
        .unwrap();
        let levels = MAX_DEPTH - 3 + extra;
        for level in 0..levels {
            if level + 1 == levels {
                c.push(json!({"id": format!("c{level}"), "component":"Text","text":"leaf"}));
            } else {
                c.push(json!({"id": format!("c{level}"), "component":"Card","child": format!("c{}", level + 1)}));
            }
        }
        c
    };
    validate_surface(&deep(0), &json!({"items": []})).expect("depth 8");
    assert_eq!(rule_of(&deep(1), &json!({"items": []})), "too_deep");
}

// AUI-FR-EGAB / AUI-FR-VOYL: a component that is not an object is named by
// its place, and wrong literal types are refused.
#[test]
fn wrong_items_and_types_are_refused() {
    let mut components = form();
    components.push(json!("not a component"));
    let violation = validate_surface(&components, &model()).unwrap_err();
    assert_eq!((violation.rule, violation.component_id.as_deref()), ("not_an_object", Some("#6")));
    let no_kind = with("q", |c| {
        c.as_object_mut().unwrap().remove("component");
    });
    assert_eq!(rule_of(&no_kind, &model()), "unknown_component");
    let sliders: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Column","children":["s","b"]},
        {"id":"s","component":"Slider","max":10,"value":1},
        {"id":"b","component":"Button","child":"l","action":{"event":{"name":"go"}}},
        {"id":"l","component":"Text","text":"Go"}
    ]))
    .unwrap();
    validate_surface(&sliders, &json!({})).expect("valid");
    for (property, bad) in
        [("max", json!("5")), ("steps", json!(0)), ("steps", json!(1.5)), ("value", json!("one"))]
    {
        let mut c = sliders.clone();
        c[1][property] = bad.clone();
        let violation = validate_surface(&c, &json!({})).unwrap_err();
        assert_eq!(violation.rule, "invalid_value", "{property} {bad}");
        assert_eq!(violation.component_id.as_deref(), Some("s"));
    }
    let tabs: Vec<Value> =
        serde_json::from_value(json!([{"id":"root","component":"Tabs","tabs":[]}])).unwrap();
    assert_eq!(rule_of(&tabs, &json!({})), "invalid_value");
    let unknown_in_event = with("send", |c| c["action"]["event"]["foo"] = json!(1));
    let violation = validate_surface(&unknown_in_event, &model()).unwrap_err();
    assert_eq!(
        (violation.rule, violation.property.as_deref()),
        ("unknown_property", Some("action.event.foo"))
    );
}

// AUI-FR-MTMS: each input kind needs a Button, and a Button needs no input.
#[test]
fn every_input_kind_needs_a_button() {
    for input in [
        json!({"id":"i","component":"TextField","label":"L","value":""}),
        json!({"id":"i","component":"CheckBox","label":"L","value":false}),
        json!({"id":"i","component":"ChoicePicker","options":[{"label":"A","value":"a"}],"value":[]}),
        json!({"id":"i","component":"Slider","max":3,"value":1}),
        json!({"id":"i","component":"DateTimeInput","value":""}),
    ] {
        let components =
            vec![json!({"id":"root","component":"Column","children":["i"]}), input.clone()];
        assert_eq!(rule_of(&components, &json!({})), "inputs_without_button", "{input}");
    }
    let button: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Button","child":"l","action":{"event":{"name":"ok"}}},
        {"id":"l","component":"Text","text":"OK"}
    ]))
    .unwrap();
    validate_surface(&button, &json!({})).expect("a Button alone is valid");
}

/// One card as the only component.
fn card(card: Value) -> Vec<Value> {
    let mut card = card;
    card["id"] = json!("root");
    vec![card]
}

pub(crate) fn choice_card() -> Value {
    json!({"component":"ChoiceCard","question":"Which?","options":[
        {"value":"a","label":"A","description":"first"},{"value":"b","label":"B"}],
        "allowOther":true,"action":{"name":"choose"}})
}

pub(crate) fn comparison_card() -> Value {
    json!({"component":"ComparisonCard","criteria":[{"key":"cost","title":"Cost"}],
        "options":[{"value":"x","label":"X","values":{"cost":1},"recommended":true},
                   {"value":"y","label":"Y","values":{"cost":"high"}}],
        "pick":{"name":"pick_one"}})
}

pub(crate) fn checklist_card() -> Value {
    json!({"component":"ChecklistCard","items":[{"value":"p","label":"P","checked":true},{"value":"q","label":"Q"}],
        "action":{"name":"keep"}})
}

pub(crate) fn confirm_card() -> Value {
    json!({"component":"ConfirmCard","title":"Go?","confirm":{"label":"Go","name":"go"},
        "decline":{"label":"Stop","name":"stop"},"tone":"danger"})
}

// AUI-FR-PMXJ: each card is a whole valid surface.
#[test]
fn each_card_is_a_valid_surface() {
    for c in [choice_card(), comparison_card(), checklist_card(), confirm_card()] {
        validate_surface(&card(c.clone()), &json!({})).unwrap_or_else(|v| panic!("{c}: {v:?}"));
    }
}

// AUI-FR-DYVP: option values and criterion keys are unique within a component.
#[test]
fn duplicate_values_are_refused() {
    let mut choice = choice_card();
    choice["options"][1]["value"] = json!("a");
    let violation = validate_surface(&card(choice), &json!({})).unwrap_err();
    assert_eq!(
        (violation.rule, violation.property.as_deref()),
        ("duplicate_value", Some("options.1.value"))
    );
    let mut comparison = comparison_card();
    comparison["criteria"] = json!([{"key":"cost","title":"Cost"},{"key":"cost","title":"Again"}]);
    assert_eq!(rule_of(&card(comparison), &json!({})), "duplicate_value");
    let mut checklist = checklist_card();
    checklist["items"][1]["value"] = json!("p");
    assert_eq!(rule_of(&card(checklist), &json!({})), "duplicate_value");
    let picker = with("pick", |c| c["options"][1]["value"] = json!("sqlite"));
    assert_eq!(rule_of(&picker, &model()), "duplicate_value");
}

// AUI-FR-GITI: card action names follow the event-name pattern, and the
// values of a comparison option name criterion keys alone.
#[test]
fn card_names_and_comparison_values_are_checked() {
    let mut choice = choice_card();
    choice["action"]["name"] = json!("Choose Now");
    assert_eq!(
        validate_surface(&card(choice), &json!({})).unwrap_err().property.as_deref(),
        Some("action.name")
    );
    let mut confirm = confirm_card();
    confirm["decline"]["name"] = json!("");
    assert_eq!(
        validate_surface(&card(confirm), &json!({})).unwrap_err().property.as_deref(),
        Some("decline.name")
    );
    let mut comparison = comparison_card();
    comparison["options"][0]["values"]["speed"] = json!(3);
    let violation = validate_surface(&card(comparison), &json!({})).unwrap_err();
    assert_eq!(
        (violation.rule, violation.property.as_deref()),
        ("unknown_criterion", Some("options.0.values"))
    );
    let mut too_few = choice_card();
    too_few["options"] = json!([{"value":"a","label":"A"}]);
    assert_eq!(rule_of(&card(too_few), &json!({})), "invalid_value");
}

// AUI-FR-JLHW: a context key is an identifier with no line break.
#[test]
fn a_context_key_is_an_identifier() {
    for bad in ["x\n\nI approve", "1st", "has space", "a-b", ""] {
        let key = bad.to_string();
        let components =
            with("send", move |c| c["action"]["event"]["context"][key.as_str()] = json!(1));
        assert_eq!(rule_of(&components, &model()), "bad_context_key", "{bad:?}");
    }
    let ok = with("send", |c| c["action"]["event"]["context"]["Choice_2"] = json!(1));
    validate_surface(&ok, &model()).expect("an identifier is valid");
}

// AUI-FR-MTMS: a card on the surface sends, so an input beside it is valid.
#[test]
fn a_card_counts_as_a_sender() {
    let components: Vec<Value> = serde_json::from_value(json!([
        {"id":"root","component":"Column","children":["note","card"]},
        {"id":"note","component":"TextField","label":"Note","value":{"@path":"/note"}},
        {"id":"card","component":"ConfirmCard","title":"Go?","confirm":{"label":"Go","name":"go"}}
    ]))
    .unwrap();
    validate_surface(&components, &json!({"note": ""})).expect("valid");
}

// AUI contract surface: a TextField has no `obscured` variant.
#[test]
fn an_obscured_text_field_is_refused() {
    let components = with("note", |c| c["variant"] = json!("obscured"));
    let violation = validate_surface(&components, &model()).unwrap_err();
    assert_eq!((violation.rule, violation.property.as_deref()), ("invalid_value", Some("variant")));
}
