//! Keyboard shortcuts come from the registry's `shortcut` strings
//! ("Cmd+Shift+Z", "Delete", "R"); the tool keys 1–6 / 8 follow pflow.xyz.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use pflow_native_core::Tool;
use serde_json::{json, Value};

use crate::App;

pub fn parse(s: &str) -> Option<KeyboardShortcut> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part {
            "Cmd" | "Ctrl" => mods |= Modifiers::COMMAND,
            "Shift" => mods |= Modifiers::SHIFT,
            "Alt" => mods |= Modifiers::ALT,
            "=" => key = Some(Key::Equals),
            "-" => key = Some(Key::Minus),
            k => key = Key::from_name(k),
        }
    }
    key.map(|k| KeyboardShortcut::new(mods, k))
}

/// "Cmd+Shift+Z" rendered for this platform (⌘⇧Z on macOS).
pub fn display(s: &str) -> String {
    if cfg!(target_os = "macos") {
        s.replace("Cmd+", "⌘")
            .replace("Shift+", "⇧")
            .replace("Alt+", "⌥")
    } else {
        s.replace("Cmd+", "Ctrl+")
    }
}

pub fn handle(app: &mut App, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let reg = app.sess.registry();
    let mut fire = vec![];
    ctx.input_mut(|i| {
        // Longest modifier sets first, so Cmd+Shift+Z is not taken as Cmd+Z.
        // Cmd+C/X/V come as clipboard events, not keys (clipboard.rs).
        let mut specs: Vec<_> = reg
            .commands()
            .iter()
            .filter(|c| !matches!(c.id, "edit.copy" | "edit.cut" | "edit.paste"))
            .filter_map(|c| c.shortcut.and_then(parse).map(|k| (k, c.id)))
            .collect();
        specs.sort_by_key(|(k, _)| {
            std::cmp::Reverse(
                k.modifiers.shift as u8 + k.modifiers.command as u8 + k.modifiers.alt as u8,
            )
        });
        for (k, id) in specs {
            if i.consume_shortcut(&k) {
                fire.push(id);
            }
        }
        if i.consume_key(Modifiers::COMMAND, Key::K) {
            fire.push("palette");
        }
        if i.consume_key(Modifiers::NONE, Key::Backspace) {
            fire.push("edit.delete");
        }
        let tools = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
        ];
        for (n, k) in tools.iter().enumerate() {
            if i.consume_key(Modifiers::NONE, *k) {
                fire.push(Tool::ALL[n].name());
            }
        }
        if i.consume_key(Modifiers::NONE, Key::Num8) || i.consume_key(Modifiers::NONE, Key::X) {
            fire.push("toggle-play");
        }
    });
    for id in fire {
        match id {
            "palette" => app.dialogs.palette_open = !app.dialogs.palette_open,
            "toggle-play" => {
                let t = if app.sess.tool == Tool::Fire {
                    "select"
                } else {
                    "fire"
                };
                app.queue("tool.set", json!({ "tool": t }));
            }
            name if Tool::parse(name).is_some() => app.queue("tool.set", json!({ "tool": name })),
            "edit.delete" if app.sess.selection.is_empty() => {
                if let Some((a, b)) = app.canvas_selected_arc() {
                    app.queue("net.delete_arc", json!({ "source": a, "target": b }));
                }
            }
            "edit.select_none" => {
                app.cancel_gesture();
                app.queue(id, Value::Null);
            }
            "sim.ode" => {
                let args = app.plot.args();
                app.queue(id, args);
            }
            "file.open" | "file.save" | "export.svg" | "view.screenshot" => app.menu(id),
            _ => app.queue(id, Value::Null),
        }
    }
}
