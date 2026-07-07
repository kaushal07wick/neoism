// Shared macros for building binding tables.
//
// `bindings!` constructs a `Vec<Binding<...>>` from a compact DSL, and
// `trigger!` translates the DSL key syntax into a concrete `BindingKey`
// (or pass-through for non-keyboard triggers such as `MouseButton`).

macro_rules! bindings {
    (
        $ty:ident;
        $(
            $key:expr
            $(=>$location:expr)?
            $(,$mods:expr)*
            $(,+$mode:expr)*
            $(,~$notmode:expr)*
            ;$action:expr
        );*
        $(;)*
    ) => {{
        let mut v = Vec::new();

        $(
            let mut _mods = ModifiersState::empty();
            $(_mods = $mods;)*
            let mut _mode = BindingMode::empty();
            $(_mode.insert($mode);)*
            let mut _notmode = BindingMode::empty();
            $(_notmode.insert($notmode);)*

            v.push($ty {
                trigger: trigger!($ty, $key, $($location)?),
                mods: _mods,
                mode: _mode,
                notmode: _notmode,
                action: $action.into(),
            });
        )*

        v
    }};
}

macro_rules! trigger {
    (KeyBinding, $key:literal, $location:expr) => {{
        BindingKey::Keycode {
            key: Character($key.into()),
            location: $location,
        }
    }};
    (KeyBinding, $key:literal,) => {{
        BindingKey::Keycode {
            key: Character($key.into()),
            location: KeyLocation::Standard,
        }
    }};
    (KeyBinding, $key:expr,) => {{
        BindingKey::Keycode {
            key: $key,
            location: KeyLocation::Standard,
        }
    }};
    ($ty:ident, $key:expr,) => {{
        $key
    }};
}

pub(crate) use bindings;
pub(crate) use trigger;
