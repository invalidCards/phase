//! Regression for Machine God's Effigy.
//!
//! Its copy replacement says `except it's an artifact`, which replaces the
//! copied creature's card types; it is not the additive `in addition to its
//! other types` form used by Copy Artifact and similar cards.

use engine::game::effects::become_copy;
use engine::game::layers::evaluate_layers;
use engine::game::mana_abilities::is_mana_ability;
use engine::game::scenario::{GameScenario, P0};
use engine::parser::oracle::parse_oracle_text;
use engine::types::ability::{
    ContinuousModification, CopyRecipient, Duration, Effect, QuantityExpr, ResolvedAbility,
    StaticDefinition, TargetFilter, TargetRef,
};
use engine::types::card_type::CoreType;
use engine::types::counter::CounterType;
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaColor, ManaCost, ManaType, ManaUnit};
use engine::types::phase::Phase;

const MACHINE_GODS_EFFIGY: &str = "You may have this artifact enter as a copy of any creature on the battlefield, except it's an artifact and it has \"{T}: Add {U}.\" (It's not a creature.)\n{T}: Add {U}.";
const COPY_ARTIFACT: &str = "You may have this enchantment enter as a copy of any artifact on the battlefield, except it's an enchantment in addition to its other types.";
const LAZOTEP_CONVERT: &str = "You may have this creature enter as a copy of any creature card in a graveyard, except it's a 4/4 black Zombie in addition to its other colors and types.";
const DEVOID: &str = "Devoid (This card has no color.)";

fn copy_exception_modifications(
    oracle: &str,
    name: &str,
    card_types: &[String],
) -> Vec<ContinuousModification> {
    let parsed = parse_oracle_text(oracle, name, &[], card_types, &[]);
    let replacement = parsed
        .replacements
        .first()
        .expect("copy-as-enters replacement must parse");
    let execute = replacement
        .execute
        .as_ref()
        .expect("copy-as-enters replacement must carry an execute ability");
    let Effect::BecomeCopy {
        additional_modifications,
        ..
    } = execute.effect.as_ref()
    else {
        panic!("replacement must execute BecomeCopy: {execute:?}");
    };
    additional_modifications.clone()
}

fn resolve_self_copy(
    state: &mut engine::types::game_state::GameState,
    recipient: ObjectId,
    donor: ObjectId,
    additional_modifications: Vec<ContinuousModification>,
) {
    let ability = ResolvedAbility::new(
        Effect::BecomeCopy {
            recipient: CopyRecipient::Source,
            target: TargetFilter::Any,
            duration: Some(Duration::Permanent),
            mana_value_limit: None,
            additional_modifications,
        },
        vec![TargetRef::Object(donor)],
        recipient,
        P0,
    );
    become_copy::resolve(state, &ability, &mut Vec::new()).expect("copy resolver succeeds");
    state.layers_dirty.mark_full();
    evaluate_layers(state);
}

/// The exact Oracle parser output used by card-data generation must distinguish
/// type replacement from the additive Copy Artifact family.
#[test]
fn copy_exception_type_modes_remain_distinct() {
    let effigy_modifications = copy_exception_modifications(
        MACHINE_GODS_EFFIGY,
        "Machine God's Effigy",
        &["Artifact".to_string()],
    );
    assert!(
        effigy_modifications.contains(&ContinuousModification::SetCardTypes {
            core_types: vec![CoreType::Artifact],
        })
    );
    assert!(
        effigy_modifications.iter().any(|modification| matches!(
            modification,
            ContinuousModification::GrantAbility { definition }
                if matches!(definition.effect.as_ref(), Effect::Mana { .. })
        )),
        "Effigy must retain its quoted blue mana ability: {effigy_modifications:?}"
    );
    assert!(
        !effigy_modifications.iter().any(|modification| matches!(
            modification,
            ContinuousModification::GrantAbility { definition }
                if matches!(definition.effect.as_ref(), Effect::Unimplemented { .. })
        )),
        "Effigy must have no unimplemented copied exception: {effigy_modifications:?}"
    );

    let copy_artifact_modifications =
        copy_exception_modifications(COPY_ARTIFACT, "Copy Artifact", &["Enchantment".to_string()]);
    assert!(
        copy_artifact_modifications.contains(&ContinuousModification::AddType {
            core_type: CoreType::Enchantment,
        })
    );
    assert!(
        !copy_artifact_modifications
            .iter()
            .any(|modification| matches!(
                modification,
                ContinuousModification::SetCardTypes { .. }
            )),
        "Copy Artifact remains additive: {copy_artifact_modifications:?}"
    );
}

/// CR 614.12a + CR 707.9b + CR 205.1a: selecting the second of two creature
/// donors as Machine God's Effigy enters copies that donor, but the exception
/// replaces Creature with Artifact and grants the blue mana ability.
#[test]
fn effigy_copies_selected_donor_as_a_noncreature_artifact_with_blue_mana() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let _first = scenario.add_creature(P0, "First Donor", 2, 2).id();
    let second = scenario.add_creature(P0, "Second Donor", 5, 4).id();
    let effigy = scenario
        .add_artifact_to_hand_from_oracle(P0, "Machine God's Effigy", MACHINE_GODS_EFFIGY)
        .with_mana_cost(ManaCost::generic(4))
        .id();
    scenario.with_mana_pool(
        P0,
        (0..4)
            .map(|index| ManaUnit::new(ManaType::Colorless, ObjectId(9_000 + index), false, vec![]))
            .collect(),
    );

    let mut runner = scenario.build();
    runner
        .cast(effigy)
        .replacement_choice(0)
        .copy_target(second)
        .resolve();

    let copied = &runner.state().objects[&effigy];
    assert_eq!(
        copied.name, "Second Donor",
        "the selected donor must be copied"
    );
    assert_eq!(copied.power, Some(5));
    assert_eq!(copied.toughness, Some(4));
    assert!(copied.card_types.core_types.contains(&CoreType::Artifact));
    assert!(
        !copied.card_types.core_types.contains(&CoreType::Creature),
        "the exact Effigy exception replaces creature with artifact"
    );
    let mana_ability = copied
        .abilities
        .iter()
        .position(is_mana_ability)
        .expect("the quoted blue mana ability must be present after copying");

    runner.activate(effigy, mana_ability).resolve();
    let state = runner.state();
    assert!(
        state.objects[&effigy].tapped,
        "the mana ability pays its tap cost"
    );
    assert_eq!(
        state.players[P0.0 as usize]
            .mana_pool
            .count_color(ManaType::Blue),
        1
    );
}

/// CR 707.2 + CR 707.9b/d: the completed Effigy copy exception is part of its
/// copiable values.  Copy Artifact therefore sees the artifact-only Effigy,
/// then adds Enchantment without reintroducing Creature.
#[test]
fn copy_artifact_snapshots_effigys_complete_type_replacement() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let donor = scenario.add_creature(P0, "Effigy Donor", 5, 4).id();
    let effigy = scenario
        .add_artifact_to_hand_from_oracle(P0, "Machine God's Effigy", MACHINE_GODS_EFFIGY)
        .with_mana_cost(ManaCost::generic(4))
        .id();
    let copy_artifact = scenario
        .add_creature_to_hand_from_oracle(P0, "Copy Artifact", 0, 0, COPY_ARTIFACT)
        .as_enchantment()
        .with_mana_cost(ManaCost::generic(2))
        .id();
    scenario.with_mana_pool(
        P0,
        (0..6)
            .map(|index| ManaUnit::new(ManaType::Colorless, ObjectId(9_100 + index), false, vec![]))
            .collect(),
    );

    let mut runner = scenario.build();
    runner
        .cast(effigy)
        .replacement_choice(0)
        .copy_target(donor)
        .resolve();
    runner
        .cast(copy_artifact)
        .replacement_choice(0)
        .copy_target(effigy)
        .resolve();

    let copied = &runner.state().objects[&copy_artifact];
    assert!(copied.card_types.core_types.contains(&CoreType::Artifact));
    assert!(copied
        .card_types
        .core_types
        .contains(&CoreType::Enchantment));
    assert!(
        !copied.card_types.core_types.contains(&CoreType::Creature),
        "Copy Artifact must snapshot Effigy's artifact-only copy exception"
    );
    let mana_ability = copied
        .abilities
        .iter()
        .position(is_mana_ability)
        .expect("a copy of Effigy retains its blue mana ability");

    runner.activate(copy_artifact, mana_ability).resolve();
    let state = runner.state();
    assert!(state.objects[&copy_artifact].tapped);
    assert_eq!(
        state.players[P0.0 as usize]
            .mana_pool
            .count_color(ManaType::Blue),
        1
    );
}

/// CR 707.9d + CR 604.3: resolver-level regression using Lazotep Convert's
/// exactly parsed copy exception. Its black exception replaces a copied Devoid
/// creature's color-defining ability, even though it adds black in addition to
/// the source's other colors and types.
#[test]
fn lazotep_convert_color_exception_does_not_copy_devoid_cda() {
    let mut scenario = GameScenario::new();
    let donor = {
        let mut builder = scenario.add_creature(P0, "Devoid Donor", 2, 3);
        builder.from_oracle_text_with_keywords(&["Devoid"], DEVOID);
        builder.id()
    };
    let recipient = scenario.add_creature(P0, "Lazotep Host", 0, 0).id();
    let mut state = scenario.build().state().clone();

    assert!(
        state.objects[&donor]
            .base_static_definitions
            .iter()
            .any(|definition| {
                definition.characteristic_defining
                    && matches!(
                        definition.modifications.as_slice(),
                        [ContinuousModification::SetColor { colors }] if colors.is_empty()
                    )
            }),
        "the test donor must carry Devoid's synthesized color CDA"
    );

    let modifications = copy_exception_modifications(
        LAZOTEP_CONVERT,
        "Lazotep Convert",
        &["Creature".to_string()],
    );
    assert!(
        modifications.contains(&ContinuousModification::AddColor {
            color: ManaColor::Black,
        }),
        "Lazotep Convert must reach the folded additive-color exception: {modifications:?}"
    );

    resolve_self_copy(&mut state, recipient, donor, modifications);

    let copied = &state.objects[&recipient];
    assert_eq!(copied.name, "Devoid Donor");
    assert_eq!((copied.power, copied.toughness), (Some(4), Some(4)));
    assert!(copied.card_types.subtypes.contains(&"Zombie".to_string()));
    assert_eq!(copied.color, vec![ManaColor::Black]);
}

/// A non-foldable rider must leave *all* layer operations on the first copy;
/// otherwise the preceding foldable rider leaks into a later vanilla copy.
#[test]
fn unsupported_copy_exception_rider_keeps_preceding_subtype_out_of_later_copy() {
    let mut scenario = GameScenario::new();
    let donor = scenario.add_creature(P0, "Fallback Donor", 2, 2).id();
    let first = scenario.add_creature(P0, "First Host", 0, 0).id();
    let second = scenario.add_creature(P0, "Second Host", 0, 0).id();
    let mut state = scenario.build().state().clone();

    resolve_self_copy(
        &mut state,
        first,
        donor,
        vec![
            ContinuousModification::AddSubtype {
                subtype: "Dog".to_string(),
            },
            ContinuousModification::AddPower { value: 3 },
        ],
    );
    assert!(state.objects[&first]
        .card_types
        .subtypes
        .contains(&"Dog".to_string()));
    assert_eq!(state.objects[&first].power, Some(5));

    resolve_self_copy(&mut state, second, first, Vec::new());
    assert!(
        !state.objects[&second]
            .card_types
            .subtypes
            .contains(&"Dog".to_string()),
        "the unsupported rider must prevent an earlier subtype fold from leaking"
    );
    assert_eq!(state.objects[&second].power, Some(2));
}

/// An unclassifiable CDA must follow the same all-or-nothing fallback: its
/// functional subtype definition remains copiable, while the noncopiable power
/// rider does not become part of a later copy's base values.
#[test]
fn unclassifiable_cda_preserves_its_functional_definition_without_power_rider() {
    let mut scenario = GameScenario::new();
    let donor = scenario.add_creature(P0, "CDA Donor", 2, 2).id();
    let first = scenario.add_creature(P0, "First CDA Host", 0, 0).id();
    let second = scenario.add_creature(P0, "Second CDA Host", 0, 0).id();
    let mut state = scenario.build().state().clone();

    let dog_cda = StaticDefinition::continuous()
        .affected(TargetFilter::SelfRef)
        .cda()
        .modifications(vec![ContinuousModification::AddSubtype {
            subtype: "Dog".to_string(),
        }]);
    let donor_object = state
        .objects
        .get_mut(&donor)
        .expect("scenario donor is on the battlefield");
    donor_object.static_definitions = vec![dog_cda.clone()].into();
    donor_object.base_static_definitions = std::sync::Arc::new(vec![dog_cda]);
    state.layers_dirty.mark_full();
    evaluate_layers(&mut state);

    resolve_self_copy(
        &mut state,
        first,
        donor,
        vec![ContinuousModification::SetPower { value: 7 }],
    );
    assert!(state.objects[&first]
        .card_types
        .subtypes
        .contains(&"Dog".to_string()));
    assert_eq!(state.objects[&first].power, Some(7));

    resolve_self_copy(&mut state, second, first, Vec::new());
    assert!(state.objects[&second]
        .card_types
        .subtypes
        .contains(&"Dog".to_string()));
    assert_eq!(state.objects[&second].power, Some(2));
}

/// Resolution-time exceptions are consumed independently of snapshot folding.
/// The first copy receives each exception, while the later vanilla copy sees
/// only the permanent no-cost and starting-loyalty values.
#[test]
fn resolution_time_copy_exceptions_survive_layered_fallback_without_leaking_riders() {
    let mut scenario = GameScenario::new();
    let donor = scenario
        .add_creature(P0, "Resolution Donor", 2, 2)
        .with_mana_cost(ManaCost::generic(3))
        .id();
    let first = scenario
        .add_creature(P0, "First Resolution Host", 0, 0)
        .id();
    let second = scenario
        .add_creature(P0, "Second Resolution Host", 0, 0)
        .id();
    let mut state = scenario.build().state().clone();
    let charge = CounterType::Generic("charge".to_string());

    resolve_self_copy(
        &mut state,
        first,
        donor,
        vec![
            ContinuousModification::RemoveManaCost,
            ContinuousModification::SetStartingLoyalty { value: 7 },
            ContinuousModification::AddCounterOnEnter {
                counter_type: charge.clone(),
                count: QuantityExpr::Fixed { value: 1 },
                if_type: Some(CoreType::Creature),
            },
            ContinuousModification::AddPower { value: 3 },
        ],
    );
    assert_eq!(state.objects[&first].mana_cost, ManaCost::NoCost);
    assert_eq!(state.objects[&first].loyalty, Some(7));
    assert_eq!(state.objects[&first].power, Some(5));
    assert_eq!(state.objects[&first].counters.get(&charge), Some(&1));

    resolve_self_copy(&mut state, second, first, Vec::new());
    assert_eq!(state.objects[&second].mana_cost, ManaCost::NoCost);
    assert_eq!(state.objects[&second].loyalty, Some(7));
    assert_eq!(state.objects[&second].power, Some(2));
    assert_eq!(state.objects[&second].counters.get(&charge), None);
}
