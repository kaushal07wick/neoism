// Tests for the bindings module. Moved out of `bindings/mod.rs` to keep
// the top-level module file slim.

use super::*;

use neoism_window::keyboard::ModifiersState;

type MockBinding = Binding<usize>;

impl Default for MockBinding {
    fn default() -> Self {
        Self {
            mods: Default::default(),
            action: Action::None,
            mode: BindingMode::empty(),
            notmode: BindingMode::empty(),
            trigger: Default::default(),
        }
    }
}

#[test]
fn binding_matches_itself() {
    let binding = MockBinding::default();
    let identical_binding = MockBinding::default();

    assert!(binding.triggers_match(&identical_binding));
    assert!(identical_binding.triggers_match(&binding));
}

#[test]
fn binding_matches_different_action() {
    let binding = MockBinding::default();
    let different_action = MockBinding {
        action: Action::ClearHistory,
        ..MockBinding::default()
    };

    assert!(binding.triggers_match(&different_action));
    assert!(different_action.triggers_match(&binding));
}

#[test]
fn mods_binding_requires_strict_match() {
    let superset_mods = MockBinding {
        mods: ModifiersState::all(),
        ..MockBinding::default()
    };
    let subset_mods = MockBinding {
        mods: ModifiersState::ALT,
        ..MockBinding::default()
    };

    assert!(!superset_mods.triggers_match(&subset_mods));
    assert!(!subset_mods.triggers_match(&superset_mods));
}

#[test]
fn binding_matches_identical_mode() {
    let b1 = MockBinding {
        mode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };
    let b2 = MockBinding {
        mode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };

    assert!(b1.triggers_match(&b2));
    assert!(b2.triggers_match(&b1));
}

#[test]
fn binding_without_mode_matches_any_mode() {
    let b1 = MockBinding::default();
    let b2 = MockBinding {
        mode: BindingMode::APP_KEYPAD,
        notmode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };

    assert!(b1.triggers_match(&b2));
}

#[test]
fn binding_with_mode_matches_empty_mode() {
    let b1 = MockBinding {
        mode: BindingMode::APP_KEYPAD,
        notmode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };
    let b2 = MockBinding::default();

    assert!(b1.triggers_match(&b2));
    assert!(b2.triggers_match(&b1));
}

#[test]
fn binding_matches_modes() {
    let b1 = MockBinding {
        mode: BindingMode::ALT_SCREEN | BindingMode::APP_KEYPAD,
        ..MockBinding::default()
    };
    let b2 = MockBinding {
        mode: BindingMode::APP_KEYPAD,
        ..MockBinding::default()
    };

    assert!(b1.triggers_match(&b2));
    assert!(b2.triggers_match(&b1));
}

#[test]
fn binding_matches_partial_intersection() {
    let b1 = MockBinding {
        mode: BindingMode::ALT_SCREEN | BindingMode::APP_KEYPAD,
        ..MockBinding::default()
    };
    let b2 = MockBinding {
        mode: BindingMode::APP_KEYPAD | BindingMode::APP_CURSOR,
        ..MockBinding::default()
    };

    assert!(b1.triggers_match(&b2));
    assert!(b2.triggers_match(&b1));
}

#[test]
fn binding_mismatches_notmode() {
    let b1 = MockBinding {
        mode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };
    let b2 = MockBinding {
        notmode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };

    assert!(!b1.triggers_match(&b2));
    assert!(!b2.triggers_match(&b1));
}

#[test]
fn binding_mismatches_unrelated() {
    let b1 = MockBinding {
        mode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };
    let b2 = MockBinding {
        mode: BindingMode::APP_KEYPAD,
        ..MockBinding::default()
    };

    assert!(!b1.triggers_match(&b2));
    assert!(!b2.triggers_match(&b1));
}

#[test]
fn binding_matches_notmodes() {
    let subset_notmodes = MockBinding {
        notmode: BindingMode::VI | BindingMode::APP_CURSOR,
        ..MockBinding::default()
    };
    let superset_notmodes = MockBinding {
        notmode: BindingMode::APP_CURSOR,
        ..MockBinding::default()
    };

    assert!(subset_notmodes.triggers_match(&superset_notmodes));
    assert!(superset_notmodes.triggers_match(&subset_notmodes));
}

#[test]
fn binding_matches_mode_notmode() {
    let b1 = MockBinding {
        mode: BindingMode::VI,
        notmode: BindingMode::APP_CURSOR,
        ..MockBinding::default()
    };
    let b2 = MockBinding {
        notmode: BindingMode::APP_CURSOR,
        ..MockBinding::default()
    };

    assert!(b1.triggers_match(&b2));
    assert!(b2.triggers_match(&b1));
}

// #[test]
// fn binding_trigger_input() {
//     let binding = MockBinding { trigger: 13, ..MockBinding::default() };

//     let mods = binding.mods;
//     let mode = binding.mode;

//     assert!(binding.is_triggered_by(mode, mods, &13));
//     assert!(!binding.is_triggered_by(mode, mods, &32));
// }

// #[test]
// fn binding_trigger_mods() {
//     let binding = MockBinding {
//         mods: ModifiersState::ALT | ModifiersState::SUPER,
//         ..MockBinding::default()
//     };

//     let superset_mods = ModifiersState::all();
//     let subset_mods = ModifiersState::empty();

//     let t = binding.trigger;
//     let mode = binding.mode;

//     assert!(binding.is_triggered_by(mode, binding.mods, &t));
//     assert!(!binding.is_triggered_by(mode, superset_mods, &t));
//     assert!(!binding.is_triggered_by(mode, subset_mods, &t));
// }

#[test]
fn binding_trigger_modes() {
    let binding = MockBinding {
        mode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };

    let t = binding.trigger;
    let mods = binding.mods;

    assert!(!binding.is_triggered_by(BindingMode::VI, mods, &t));
    assert!(binding.is_triggered_by(BindingMode::ALT_SCREEN, mods, &t));
    assert!(binding.is_triggered_by(BindingMode::ALT_SCREEN | BindingMode::VI, mods, &t));
}

#[test]
fn binding_trigger_notmodes() {
    let binding = MockBinding {
        notmode: BindingMode::ALT_SCREEN,
        ..MockBinding::default()
    };

    let t = binding.trigger;
    let mods = binding.mods;

    assert!(binding.is_triggered_by(BindingMode::VI, mods, &t));
    assert!(!binding.is_triggered_by(BindingMode::ALT_SCREEN, mods, &t));
    assert!(!binding.is_triggered_by(
        BindingMode::ALT_SCREEN | BindingMode::VI,
        mods,
        &t
    ));
}

#[test]
fn bindings_overwrite() {
    let bindings = bindings!(
        KeyBinding;
        "q", ModifiersState::SUPER; Action::Quit;
        ",", ModifiersState::SUPER; Action::ConfigEditor;
    );

    let config_bindings = vec![ConfigKeyBinding {
        key: String::from("q"),
        action: String::from("receivechar"),
        with: String::from("super"),
        esc: String::from(""),
        mode: String::from(""),
    }];

    let new_bindings = config_key_bindings(config_bindings, bindings);

    assert_eq!(new_bindings.len(), 2);
    assert_eq!(new_bindings[1].action, Action::ReceiveChar);
}

#[test]
fn bindings_conflict_resolution() {
    // Test that conflicting bindings are properly replaced
    let bindings = bindings!(
        KeyBinding;
        Key::Named(PageUp), ModifiersState::empty(); Action::Esc("\x1b[5~".into());
        Key::Named(PageDown), ModifiersState::empty(); Action::Esc("\x1b[6~".into());
    );

    // User wants to use PageUp/PageDown for scrolling
    let config_bindings = vec![
        ConfigKeyBinding {
            key: String::from("pageup"),
            action: String::from("scroll(1)"),
            with: String::from(""),
            esc: String::from(""),
            mode: String::from(""),
        },
        ConfigKeyBinding {
            key: String::from("pagedown"),
            action: String::from("scroll(-1)"),
            with: String::from(""),
            esc: String::from(""),
            mode: String::from(""),
        },
    ];

    let new_bindings = config_key_bindings(config_bindings, bindings);

    // Should have 2 bindings (the original defaults should be replaced)
    assert_eq!(new_bindings.len(), 2);

    // Check that the actions were updated to scroll actions
    let has_scroll_actions = new_bindings
        .iter()
        .any(|b| matches!(b.action, Action::Scroll(_)));
    assert!(has_scroll_actions);
}

#[test]
fn bindings_alt_enter_conflict_resolution() {
    // Test Windows Alt+Enter conflict resolution
    let bindings = bindings!(
        KeyBinding;
        Key::Named(Enter), ModifiersState::ALT; Action::ToggleFullscreen;
    );

    // User wants to use Alt+Enter for a custom action
    let config_bindings = vec![ConfigKeyBinding {
        key: String::from("return"),
        action: String::from("scroll(1)"),
        with: String::from("alt"),
        esc: String::from(""),
        mode: String::from(""),
    }];

    let new_bindings = config_key_bindings(config_bindings, bindings);

    // Should have 1 binding (the original Alt+Enter should be replaced)
    assert_eq!(new_bindings.len(), 1);

    assert_eq!(&new_bindings[0].action, &Action::Scroll(1));
}
