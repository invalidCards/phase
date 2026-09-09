//! End-to-end regressions for event-relative damage prevention formulas.
//!
//! These tests seed the printed Oracle text, then drive the normal damage or
//! cast pipeline. They deliberately distinguish replacement choice authority
//! from the affected player's replacement ordering authority.

use engine::game::combat::AttackTarget;
use engine::game::effects::deal_damage;
use engine::game::game_object::AttachTarget;
use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::parser::oracle::parse_oracle_text;
use engine::types::ability::{
    DamageModification, Effect, QuantityExpr, ReplacementDefinition, ResolvedAbility, TargetFilter,
    TargetRef,
};
use engine::types::actions::GameAction;
use engine::types::card_type::CoreType;
use engine::types::counter::CounterType;
use engine::types::game_state::WaitingFor;
use engine::types::identifiers::ObjectId;
use engine::types::mana::ManaCost;
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::replacements::ReplacementEvent;

const GISELA: &str =
    "If a source would deal damage to you or a permanent you control, prevent half that damage, rounded up.";
const BATTLETIDE: &str =
    "If a source would deal damage to a player, you may prevent X of that damage, where X is the number of Clerics you control.";
const REM: &str =
    "If a spell would deal damage to you or another permanent you control, prevent that damage.";
const PLATED_PEGASUS: &str =
    "If a spell would deal damage to a permanent or player, prevent 1 damage that spell would deal to that permanent or player.";
const SHIELD_OF_THE_RIGHTEOUS: &str =
    "If a source would deal damage to equipped creature, prevent X of that damage, where X is the number of creatures you control.";
const COVER_OF_WINTER: &str = "Cumulative upkeep {S} (At the beginning of your upkeep, put an age counter on this permanent, then sacrifice it unless you pay its upkeep cost for each age counter on it. {S} can be paid with one mana from a snow source.)\nIf a creature would deal combat damage to you and/or one or more creatures you control, prevent X of that damage, where X is the number of age counters on this enchantment.\n{S}: Put an age counter on this enchantment.";
const BENEVOLENT_UNICORN: &str =
    "If a spell would deal damage to a permanent or player, it deals that much damage minus 1 to that permanent or player instead.";
const DAMAGE_SPELL: &str = "This spell deals 3 damage to target creature or player.";

fn damage_ability(
    source_id: ObjectId,
    controller: PlayerId,
    target: TargetRef,
    amount: i32,
) -> ResolvedAbility {
    ResolvedAbility::new(
        Effect::DealDamage {
            amount: QuantityExpr::Fixed { value: amount },
            target: TargetFilter::Any,
            damage_source: None,
            excess: None,
        },
        vec![target],
        source_id,
        controller,
    )
}

fn set_priority(runner: &mut GameRunner, player: PlayerId) {
    let state = runner.state_mut();
    state.active_player = player;
    state.priority_player = player;
    state.waiting_for = WaitingFor::Priority { player };
}

fn choose_source_candidate(runner: &mut GameRunner, source: ObjectId) {
    let index = runner
        .state()
        .pending_replacement
        .as_ref()
        .expect("damage replacement choice must be parked")
        .candidates
        .iter()
        .position(|candidate| candidate.source == source)
        .expect("the requested replacement source must be offered");
    runner
        .act(GameAction::ChooseReplacement { index })
        .expect("choosing the requested replacement must succeed");
}

#[test]
fn gisela_rounds_up_and_affected_player_orders_against_a_doubler() {
    let mut scenario = GameScenario::new();
    let gisela = scenario
        .add_creature_from_oracle(P0, "Gisela, Blade of Goldnight", 5, 5, GISELA)
        .id();
    let doubler = scenario.add_creature(P1, "Damage Doubler", 2, 2).id();
    let source = scenario.add_creature(P1, "Damage Source", 3, 3).id();
    let mut runner = scenario.build();
    runner
        .state_mut()
        .objects
        .get_mut(&doubler)
        .unwrap()
        .replacement_definitions
        .push(
            ReplacementDefinition::new(ReplacementEvent::DamageDone)
                .damage_modification(DamageModification::Double),
        );

    let before = runner.life(P0);
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(source, P1, TargetRef::Player(P0), 5),
        &mut events,
    )
    .expect("damage must reach replacement processing");

    match runner.state().waiting_for {
        WaitingFor::ReplacementChoice { player, .. } => assert_eq!(
            player, P0,
            "the damaged player, rather than a replacement controller, orders noncommuting replacements"
        ),
        ref other => panic!("expected a material replacement ordering choice, got {other:?}"),
    }
    choose_source_candidate(&mut runner, gisela);
    assert_eq!(
        runner.life(P0),
        before - 4,
        "preventing ceil(5 / 2) first leaves 2 damage, then the doubler makes 4"
    );

    let mut scenario = GameScenario::new();
    let gisela = scenario
        .add_creature_from_oracle(P0, "Gisela, Blade of Goldnight", 5, 5, GISELA)
        .id();
    let doubler = scenario.add_creature(P1, "Damage Doubler", 2, 2).id();
    let source = scenario.add_creature(P1, "Damage Source", 3, 3).id();
    let mut runner = scenario.build();
    runner
        .state_mut()
        .objects
        .get_mut(&doubler)
        .unwrap()
        .replacement_definitions
        .push(
            ReplacementDefinition::new(ReplacementEvent::DamageDone)
                .damage_modification(DamageModification::Double),
        );
    let before = runner.life(P0);
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(source, P1, TargetRef::Player(P0), 5),
        &mut events,
    )
    .expect("damage must reach replacement processing");
    choose_source_candidate(&mut runner, doubler);
    assert_eq!(
        runner.life(P0),
        before - 5,
        "doubling first makes 10 damage, then Gisela prevents 5 rounded up"
    );
    assert!(
        runner.state().objects[&gisela]
            .replacement_definitions
            .len()
            == 1,
        "reach guard: Gisela's printed static replacement must be present"
    );
}

#[test]
fn battletide_controller_chooses_optional_prevention_after_affected_player_orders() {
    let mut scenario = GameScenario::new();
    let battletide = scenario
        .add_creature_from_oracle(P0, "Battletide Alchemist", 3, 4, BATTLETIDE)
        .with_subtypes(vec!["Cleric"])
        .id();
    scenario
        .add_creature(P0, "Supporting Cleric", 1, 3)
        .with_subtypes(vec!["Cleric"]);
    let doubler = scenario.add_creature(P1, "Damage Doubler", 2, 2).id();
    let source = scenario.add_creature(P1, "Damage Source", 3, 3).id();
    let mut runner = scenario.build();
    runner
        .state_mut()
        .objects
        .get_mut(&doubler)
        .unwrap()
        .replacement_definitions
        .push(
            ReplacementDefinition::new(ReplacementEvent::DamageDone)
                .damage_modification(DamageModification::Double),
        );
    set_priority(&mut runner, P0);

    let before = runner.life(P1);
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(source, P1, TargetRef::Player(P1), 5),
        &mut events,
    )
    .expect("damage must reach replacement processing");
    match runner.state().waiting_for {
        WaitingFor::ReplacementChoice { player, .. } => assert_eq!(
            player, P1,
            "the affected player orders Battletide and the doubler under CR 616"
        ),
        ref other => panic!("expected ordering prompt, got {other:?}"),
    }
    choose_source_candidate(&mut runner, doubler);
    match runner.state().waiting_for {
        WaitingFor::ReplacementChoice { player, .. } => assert_eq!(
            player, P0,
            "Battletide's optional accept/decline belongs to its controller, not the damaged player"
        ),
        ref other => panic!("expected Battletide optional prompt, got {other:?}"),
    }
    runner
        .act(GameAction::ChooseReplacement { index: 0 })
        .expect("Battletide controller can accept prevention");
    assert_eq!(
        runner.life(P1),
        before - 8,
        "two live Clerics prevent 2 from the doubled 10-damage event"
    );
    assert!(
        runner.state().objects[&battletide]
            .replacement_definitions
            .len()
            == 1,
        "reach guard: Battletide's printed static replacement must be present"
    );
}

#[test]
fn battletide_decline_leaves_the_original_damage_untouched() {
    let mut scenario = GameScenario::new();
    scenario
        .add_creature_from_oracle(P0, "Battletide Alchemist", 3, 4, BATTLETIDE)
        .with_subtypes(vec!["Cleric"]);
    scenario
        .add_creature(P0, "Supporting Cleric", 1, 3)
        .with_subtypes(vec!["Cleric"]);
    let source = scenario.add_creature(P1, "Damage Source", 3, 3).id();
    let mut runner = scenario.build();
    set_priority(&mut runner, P0);
    let before = runner.life(P1);
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(source, P1, TargetRef::Player(P1), 5),
        &mut events,
    )
    .expect("damage must reach the optional replacement");
    match runner.state().waiting_for {
        WaitingFor::ReplacementChoice { player, .. } => assert_eq!(player, P0),
        ref other => panic!("expected Battletide controller prompt, got {other:?}"),
    }
    runner
        .act(GameAction::ChooseReplacement { index: 1 })
        .expect("Battletide controller can decline prevention");
    assert_eq!(runner.life(P1), before - 5);
}

#[test]
fn rem_and_plated_apply_only_to_spells_and_keep_their_recipient_scopes() {
    let mut scenario = GameScenario::new();
    let rem = scenario
        .add_creature_from_oracle(P0, "Rem Karolus, Stalwart Slayer", 3, 4, REM)
        .id();
    let ally = scenario.add_creature(P0, "Protected Ally", 1, 5).id();
    let spell_to_rem = scenario
        .add_spell_to_hand_from_oracle(P1, "Spell to Rem", true, DAMAGE_SPELL)
        .with_mana_cost(ManaCost::generic(0))
        .id();
    let spell_to_ally = scenario
        .add_spell_to_hand_from_oracle(P1, "Spell to Ally", true, DAMAGE_SPELL)
        .with_mana_cost(ManaCost::generic(0))
        .id();
    let permanent_source = scenario.add_creature(P1, "Permanent Source", 3, 3).id();
    let mut runner = scenario.build();
    set_priority(&mut runner, P1);
    runner.cast(spell_to_rem).target_object(rem).resolve();
    assert_eq!(
        runner.state().objects[&rem].damage_marked,
        3,
        "Rem's 'another permanent' clause must exclude Rem itself"
    );
    set_priority(&mut runner, P1);
    runner.cast(spell_to_ally).target_object(ally).resolve();
    assert_eq!(
        runner.state().objects[&ally].damage_marked,
        0,
        "a spell's damage to another permanent the controller owns is prevented"
    );
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(permanent_source, P1, TargetRef::Player(P0), 3),
        &mut events,
    )
    .expect("permanent-source damage must resolve");
    assert_eq!(
        runner.life(P0),
        17,
        "Rem must not prevent damage from a permanent or ability source"
    );

    let mut scenario = GameScenario::new();
    scenario.add_creature_from_oracle(P0, "Plated Pegasus", 1, 1, PLATED_PEGASUS);
    let spell = scenario
        .add_spell_to_hand_from_oracle(P1, "Spell", true, DAMAGE_SPELL)
        .with_mana_cost(ManaCost::generic(0))
        .id();
    let permanent_source = scenario.add_creature(P1, "Permanent Source", 3, 3).id();
    let mut runner = scenario.build();
    set_priority(&mut runner, P1);
    let before = runner.life(P0);
    let outcome = runner.cast(spell).target_player(P0).resolve();
    assert_eq!(
        outcome.life_delta(P0),
        -2,
        "Plated Pegasus prevents one spell damage"
    );
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(permanent_source, P1, TargetRef::Player(P0), 3),
        &mut events,
    )
    .expect("permanent-source damage must resolve");
    assert_eq!(
        runner.life(P0),
        before - 5,
        "Plated Pegasus must not affect nonspells"
    );
}

#[test]
fn shield_formula_uses_the_equipped_recipient_and_live_creature_count() {
    let mut scenario = GameScenario::new();
    let shield = scenario
        .add_creature_from_oracle(P0, "Shield", 0, 1, SHIELD_OF_THE_RIGHTEOUS)
        .id();
    let equipped = scenario.add_creature(P0, "Equipped", 2, 7).id();
    let unrelated = scenario.add_creature(P0, "Unrelated", 2, 7).id();
    let source = scenario.add_creature(P1, "Damage Source", 3, 3).id();
    let mut runner = scenario.build();
    {
        let object = runner.state_mut().objects.get_mut(&shield).unwrap();
        object.card_types.core_types = vec![CoreType::Artifact];
        object.card_types.subtypes = vec!["Equipment".to_string()];
        object.base_card_types = object.card_types.clone();
        object.power = None;
        object.toughness = None;
        object.base_power = None;
        object.base_toughness = None;
        object.attached_to = Some(AttachTarget::Object(equipped));
    }
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(source, P1, TargetRef::Object(equipped), 5),
        &mut events,
    )
    .expect("damage to equipped creature must resolve");
    assert_eq!(runner.state().objects[&equipped].damage_marked, 3);
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(source, P1, TargetRef::Object(unrelated), 5),
        &mut events,
    )
    .expect("damage to unrelated creature must resolve");
    assert_eq!(runner.state().objects[&unrelated].damage_marked, 5);
}

/// CR 120.2a + CR 615.1a: Cover of Winter's continuous prevention formula
/// applies to combat damage dealt to a creature its controller controls, but
/// not to otherwise-identical noncombat damage.
#[test]
fn cover_of_winter_prevents_live_age_counter_formula_only_for_combat_damage() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let cover = scenario
        .add_enchantment_from_oracle(P0, "Cover of Winter", COVER_OF_WINTER)
        .id();
    scenario.with_counter(cover, CounterType::Age, 2);
    let protected_creature = scenario.add_creature(P0, "Protected Creature", 1, 10).id();
    let attacker = scenario.add_creature(P1, "Hostile Attacker", 4, 6).id();
    let mut runner = scenario.build();
    set_priority(&mut runner, P1);

    runner.advance_to_combat();
    runner
        .declare_attackers(&[(attacker, AttackTarget::Player(P0))])
        .expect("attacker declaration must succeed");
    if matches!(runner.state().waiting_for, WaitingFor::Priority { .. }) {
        runner.pass_both_players();
    }
    runner
        .declare_blockers(&[(protected_creature, attacker)])
        .expect("blocker declaration must succeed");
    runner.combat_damage();

    assert_eq!(
        runner.state().objects[&protected_creature].damage_marked,
        2,
        "two age counters must prevent two of the attacker's four combat damage"
    );
    assert!(
        runner.state().objects[&cover].replacement_definitions.len() == 1,
        "reach guard: Cover of Winter's printed static replacement must be present"
    );

    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(attacker, P1, TargetRef::Object(protected_creature), 4),
        &mut events,
    )
    .expect("noncombat damage must reach the production damage pipeline");
    assert_eq!(
        runner.state().objects[&protected_creature].damage_marked,
        6,
        "the same creature's noncombat damage must not match Cover of Winter's combat-only shield"
    );
}

#[test]
fn benevolent_unicorn_minus_one_stays_spell_only_and_is_not_prevention() {
    let mut scenario = GameScenario::new();
    scenario.add_creature_from_oracle(P0, "Benevolent Unicorn", 1, 2, BENEVOLENT_UNICORN);
    let spell = scenario
        .add_spell_to_hand_from_oracle(P1, "Spell", true, DAMAGE_SPELL)
        .with_mana_cost(ManaCost::generic(0))
        .id();
    let permanent_source = scenario.add_creature(P1, "Permanent Source", 3, 3).id();
    let mut runner = scenario.build();
    set_priority(&mut runner, P1);
    let outcome = runner.cast(spell).target_player(P0).resolve();
    assert_eq!(outcome.life_delta(P0), -2, "the spell is reduced by one");
    assert!(
        !outcome.events().iter().any(|event| matches!(
            event,
            engine::types::events::GameEvent::DamagePrevented { .. }
        )),
        "arithmetic minus one must not emit prevention bookkeeping"
    );
    let mut events = Vec::new();
    deal_damage::resolve(
        runner.state_mut(),
        &damage_ability(permanent_source, P1, TargetRef::Player(P0), 3),
        &mut events,
    )
    .expect("permanent-source damage must resolve");
    assert_eq!(
        runner.life(P0),
        15,
        "Benevolent Unicorn must not reduce nonspell damage"
    );
}

#[test]
fn unsupported_event_relative_prevention_cards_remain_named_prevent_gaps() {
    for (name, oracle, types) in [
        (
            "Dark Sphere",
            "{T}, Sacrifice ~: The next time a source of your choice would deal damage to you this turn, prevent half that damage, rounded down.",
            &["Artifact"][..],
        ),
        (
            "Tornellan Protector",
            "{T}: Until end of turn, each time damage is dealt to target creature or player, prevent X of that damage, where X is a number from 1 to 3 chosen at random each time.",
            &["Creature"][..],
        ),
    ] {
        let types: Vec<String> = types.iter().map(|ty| (*ty).to_string()).collect();
        let parsed = parse_oracle_text(oracle, name, &[], &types, &[]);
        let effects = parsed
            .abilities
            .iter()
            .map(|ability| ability.effect.as_ref())
            .chain(
                parsed
                    .triggers
                    .iter()
                    .filter_map(|trigger| trigger.execute.as_ref().map(|ability| ability.effect.as_ref())),
            )
            .collect::<Vec<_>>();
        assert!(
            effects
                .iter()
                .copied()
                .any(|effect| matches!(effect, Effect::Unimplemented { name: gap, .. } if gap == "prevent")),
            "{name} must preserve its unsupported prevention clause as an honest named gap: {parsed:#?}"
        );
        assert!(
            effects.iter().copied().all(|effect| !matches!(
                effect,
                Effect::PreventDamage {
                    amount: engine::types::ability::PreventionAmount::Next(1),
                    ..
                }
            )),
            "{name} must not retain a fallback one-damage prevention effect: {parsed:#?}"
        );
    }
}
