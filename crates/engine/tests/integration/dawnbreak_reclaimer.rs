//! Regression coverage for Dawnbreak Reclaimer's reciprocal graveyard choices.
//!
//! The inline setup deliberately uses the parsed Oracle instruction rather
//! than a card-data fixture: the test exercises the production parser,
//! resolution continuation, and `GameAction` round trips without coupling this
//! focused engine fixture to an external card export.

use engine::game::ability_utils::build_resolved_from_def;
use engine::game::effects::resolve_ability_chain;
use engine::game::scenario::{GameScenario, P0, P1};
use engine::parser::oracle_effect::parse_effect_chain;
use engine::types::ability::AbilityKind;
use engine::types::actions::GameAction;
use engine::types::game_state::WaitingFor;
use engine::types::mana::ManaColor;
use engine::types::phase::Phase;
use engine::types::zones::Zone;

const ORACLE: &str = "Choose a creature card in an opponent's graveyard, then that player chooses a creature card in your graveyard. You may return those cards to the battlefield under their owners' control.";

#[test]
fn dawnbreak_reclaimer_binds_the_second_choice_to_the_first_cards_owner() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let first = scenario
        .add_creature_to_graveyard(P1, "Opponent Graveyard Creature", 2, 2)
        .id();
    let second = scenario
        .add_creature_to_graveyard(P0, "Controller Graveyard Creature", 3, 3)
        .id();
    let source = scenario.add_basic_land(P0, ManaColor::White);
    let mut runner = scenario.build();

    let definition = parse_effect_chain(ORACLE, AbilityKind::Spell);
    let ability = build_resolved_from_def(&definition, source, P0);
    let mut events = Vec::new();
    resolve_ability_chain(runner.state_mut(), &ability, &mut events, 0)
        .expect("Dawnbreak's first choice reaches the interactive resolver");

    match &runner.state().waiting_for {
        WaitingFor::ChooseFromZoneChoice { player, cards, .. } => {
            assert_eq!(*player, P0);
            assert_eq!(cards, &vec![first]);
        }
        other => panic!("expected the first graveyard choice, got {other:?}"),
    }
    runner
        .act(GameAction::SelectCards { cards: vec![first] })
        .expect("the controller selects the opponent-owned creature");

    match &runner.state().waiting_for {
        WaitingFor::ChooseFromZoneChoice { player, cards, .. } => {
            assert_eq!(*player, P1, "the first card's owner chooses next");
            assert_eq!(cards, &vec![second]);
        }
        other => panic!("expected the reciprocal graveyard choice, got {other:?}"),
    }
    runner
        .act(GameAction::SelectCards {
            cards: vec![second],
        })
        .expect("the first card's owner selects from the controller's graveyard");
    assert!(matches!(
        runner.state().waiting_for,
        WaitingFor::OptionalEffectChoice { player: P0, .. }
    ));

    runner
        .act(GameAction::DecideOptionalEffect { accept: true })
        .expect("the controller may return both selected cards");
    for (card, owner) in [(first, P1), (second, P0)] {
        let object = &runner.state().objects[&card];
        assert_eq!(object.zone, Zone::Battlefield);
        assert_eq!(object.controller, owner);
    }
}
