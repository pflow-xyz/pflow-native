//! The system clipboard, in the pflow clipboard format
//! (`docs/clipboard-format.md`, shared with pflow.xyz). Copy and Cut put
//! `edit.copy` / `edit.cut`'s `clip` text on it (`App::run`); Paste reads
//! its text and runs `edit.paste {clip}` for a pflow clip, falling back to
//! the session clipboard otherwise.
//!
//! The platforms deliver clipboard input differently, so [`route_keys`]
//! (from `App::raw_input_hook`) normalizes it before egui sees it:
//!
//! - Native: eframe runs with `clipboard_shortcuts: false` (main.rs). By
//!   default egui-winit turns Cmd+C/X/V into Copy/Cut/Paste events and
//!   drops the key, and Cmd+V with no text on the system clipboard yields
//!   no event at all, so a paste could not fall back to the session clip
//!   (nor did the registry's Cmd+C/X/V shortcuts ever fire). Here Cmd+C/X
//!   become Copy/Cut events as before, and Cmd+V asks eframe to read the
//!   clipboard (`ViewportCommand::RequestPaste`); its Paste event arrives
//!   two frames later, or not at all.
//! - Web: the browser's copy/cut/paste events carry the clipboard (a page
//!   may only write it inside one), and the keys arrive as well. A key
//!   counts only when no such event came with it; eframe ignores
//!   RequestPaste there, so Edit → Paste pastes the session clip.
//!
//! While a text field has focus the events are its own (egui's TextEdit
//! copies and pastes text); the net acts on them only otherwise.

use eframe::egui::{Event, Key, Modifiers};
use pflow_native_core::commands::clipboard::{parse_clip, ClipError};
use serde_json::{json, Value};

/// Whether eframe can read the system clipboard on request.
pub(crate) const NATIVE: bool = cfg!(not(target_arch = "wasm32"));

/// Frames a paste waits for the system clipboard's text. eframe turns a
/// RequestPaste sent in frame F into an action after F, runs it after
/// painting F+1, and the Paste event is in F+2's input; by F+3 none is
/// coming (the clipboard holds no text).
const PASTE_WAIT: u64 = 3;

/// egui-winit's clipboard shortcuts, which a `clipboard_shortcuts: false`
/// build recognizes itself.
fn is_cut(m: Modifiers, k: Key) -> bool {
    k == Key::Cut
        || (m.command_only() && k == Key::X)
        || (cfg!(target_os = "windows") && m.shift && k == Key::Delete)
}

fn is_copy(m: Modifiers, k: Key) -> bool {
    k == Key::Copy
        || (m.command_only() && k == Key::C)
        || (cfg!(target_os = "windows") && m.ctrl && k == Key::Insert)
}

fn is_paste(m: Modifiers, k: Key) -> bool {
    k == Key::Paste
        || (m.command_only() && k == Key::V)
        || (cfg!(target_os = "windows") && m.shift && k == Key::Insert)
}

/// Rewrites one frame's raw events: clipboard key presses become Copy and
/// Cut events (natively always, on the web unless the browser sent one) and
/// paste presses are taken out. Returns whether a paste key was pressed.
pub(crate) fn route_keys(events: &mut Vec<Event>, native: bool) -> bool {
    let had_copy = events.contains(&Event::Copy);
    let had_cut = events.contains(&Event::Cut);
    let mut paste = false;
    let mut out = Vec::with_capacity(events.len());
    for e in std::mem::take(events) {
        let Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = e
        else {
            out.push(e);
            continue;
        };
        if is_cut(modifiers, key) {
            if native || !had_cut {
                out.push(Event::Cut);
            }
        } else if is_copy(modifiers, key) {
            if native || !had_copy {
                out.push(Event::Copy);
            }
        } else if is_paste(modifiers, key) {
            paste = true;
        } else {
            out.push(e);
        }
    }
    *events = out;
    paste
}

/// What a paste of `text` (the system clipboard's, if any) should do.
#[derive(Debug, PartialEq)]
pub(crate) enum Paste {
    /// A pflow clip or net: `edit.paste {clip}`.
    Clip(String),
    /// A pflow clip this build cannot paste (a later version, or broken).
    Refuse(String),
    /// Not a pflow clip, or no text: the session clipboard.
    Session,
}

pub(crate) fn decide(text: Option<&str>) -> Paste {
    let Some(text) = text else {
        return Paste::Session;
    };
    match parse_clip(text) {
        Ok(_) => Paste::Clip(text.to_string()),
        Err(ClipError::NotAClip(_)) => Paste::Session,
        Err(e) => Paste::Refuse(e.to_string()),
    }
}

/// What one pass asks of the app.
#[derive(Debug, PartialEq)]
pub(crate) enum Act {
    Copy,
    Cut,
    /// `edit.paste` with these args.
    Paste(Value),
    Refuse(String),
    /// Keep frames coming while a paste waits.
    Repaint,
}

/// One pass's view of the clipboard: its clipboard events and focus.
pub(crate) struct Pass<'a> {
    pub events: &'a [Event],
    pub frame: u64,
    /// A text field has keyboard focus.
    pub text_focus: bool,
    /// Some label text is selected (egui copies that on Copy).
    pub label_selection: bool,
    /// The net has selected nodes.
    pub has_selection: bool,
    pub native: bool,
}

#[derive(Default)]
pub(crate) struct ClipState {
    /// A paste key was pressed this frame.
    key_paste: bool,
    /// Ask eframe to read the system clipboard at the end of this pass.
    read_os: bool,
    /// A net paste is waiting for the clipboard's text, since this frame.
    awaiting: Option<u64>,
    frame: u64,
}

impl ClipState {
    /// A paste key (from [`route_keys`]). Natively the clipboard is read
    /// either way, since a focused text field needs the text too.
    pub(crate) fn paste_key(&mut self, native: bool) {
        self.key_paste = true;
        self.read_os |= native;
    }

    /// Edit → Paste (menu, palette): always the net, whatever has focus.
    pub(crate) fn paste_command(&mut self, native: bool) {
        self.awaiting.get_or_insert(self.frame);
        self.read_os |= native;
    }

    /// Whether to send RequestPaste now (once per request).
    pub(crate) fn take_read_os(&mut self) -> bool {
        std::mem::take(&mut self.read_os)
    }

    pub(crate) fn on_pass(&mut self, p: Pass) -> Vec<Act> {
        self.frame = p.frame;
        let mut out = vec![];
        if std::mem::take(&mut self.key_paste) && !p.text_focus {
            self.awaiting.get_or_insert(p.frame);
        }
        if !p.text_focus && !p.label_selection && p.has_selection {
            if p.events.contains(&Event::Cut) {
                out.push(Act::Cut);
            } else if p.events.contains(&Event::Copy) {
                out.push(Act::Copy);
            }
        }
        let text = p.events.iter().find_map(|e| match e {
            Event::Paste(t) => Some(t.as_str()),
            _ => None,
        });
        let image = p.events.iter().any(|e| matches!(e, Event::PasteImage(_)));
        // Natively a Paste event only answers a request; on the web the
        // browser's own Edit → Paste sends one unasked.
        let unasked = !p.native && !p.text_focus;
        match (text, self.awaiting) {
            (Some(t), Some(_)) => out.push(self.paste(Some(t))),
            (Some(t), None) if unasked => out.push(self.paste(Some(t))),
            (None, Some(since)) if image || p.frame >= since + PASTE_WAIT => {
                out.push(self.paste(None))
            }
            (None, Some(_)) => out.push(Act::Repaint),
            _ => {}
        }
        out
    }

    fn paste(&mut self, text: Option<&str>) -> Act {
        self.awaiting = None;
        match decide(text) {
            Paste::Clip(t) => Act::Paste(json!({ "clip": t })),
            Paste::Session => Act::Paste(Value::Null),
            Paste::Refuse(why) => Act::Refuse(why),
        }
    }
}

/// "2 places, 1 transition and 2 arcs" from edit.copy's counts.
pub(crate) fn counts(v: &Value) -> String {
    let n = |k: &str| v[k].as_u64().unwrap_or(0);
    let one = |k: &str, name: &str| {
        let n = n(k);
        format!("{n} {name}{}", if n == 1 { "" } else { "s" })
    };
    format!(
        "{}, {} and {}",
        one("places", "place"),
        one("transitions", "transition"),
        one("arcs", "arc")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../core/tests/fixtures/clip-v1.json");

    fn key(key: Key, modifiers: Modifiers, pressed: bool) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }
    }

    fn pass(events: &[Event], frame: u64) -> Pass<'_> {
        Pass {
            events,
            frame,
            text_focus: false,
            label_selection: false,
            has_selection: true,
            native: true,
        }
    }

    #[test]
    fn native_keys_become_clipboard_events() {
        let cmd = Modifiers::COMMAND;
        let mut ev = vec![
            key(Key::C, cmd, true),
            key(Key::X, cmd, true),
            key(Key::A, cmd, true),
        ];
        assert!(!route_keys(&mut ev, true));
        assert_eq!(ev, [Event::Copy, Event::Cut, key(Key::A, cmd, true)]);
        let mut ev = vec![key(Key::V, cmd, true), key(Key::V, cmd, false)];
        assert!(
            route_keys(&mut ev, true),
            "a paste is noted even with nothing to read"
        );
        assert_eq!(
            ev,
            [key(Key::V, cmd, false)],
            "the press is taken, the release kept"
        );
        let mut ev = vec![
            key(Key::C, cmd | Modifiers::SHIFT, true),
            key(Key::C, Modifiers::NONE, true),
        ];
        let before = ev.clone();
        assert!(!route_keys(&mut ev, true));
        assert_eq!(ev, before, "Cmd+Shift+C and a bare C are not Copy");
    }

    #[test]
    fn web_keys_defer_to_the_browsers_events() {
        let cmd = Modifiers::COMMAND;
        let mut ev = vec![key(Key::C, cmd, true), Event::Copy];
        route_keys(&mut ev, false);
        assert_eq!(ev, [Event::Copy], "one Copy, not two");
        let mut ev = vec![key(Key::X, cmd, true)];
        route_keys(&mut ev, false);
        assert_eq!(ev, [Event::Cut], "no browser event: the key stands in");
        let mut ev = vec![key(Key::V, cmd, true), Event::Paste("x".into())];
        assert!(route_keys(&mut ev, false));
        assert_eq!(ev, [Event::Paste("x".into())]);
    }

    #[test]
    fn decide_follows_the_reader_rules() {
        assert_eq!(decide(Some(FIXTURE)), Paste::Clip(FIXTURE.into()));
        assert_eq!(decide(Some("hello")), Paste::Session);
        assert_eq!(decide(None), Paste::Session);
        let mut v: Value = serde_json::from_str(FIXTURE).unwrap();
        v["pflowClip"]["version"] = json!(2);
        assert!(
            matches!(decide(Some(&v.to_string())), Paste::Refuse(m) if m.contains("version 2"))
        );
        v["pflowClip"]["version"] = json!(1);
        v["arcs"][0]["target"] = json!("Done");
        assert!(
            matches!(decide(Some(&v.to_string())), Paste::Refuse(m) if m.contains("two places"))
        );
    }

    #[test]
    fn a_native_paste_waits_for_the_clipboard_then_falls_back() {
        let mut st = ClipState::default();
        st.paste_key(true);
        assert_eq!(st.on_pass(pass(&[], 10)), [Act::Repaint]);
        assert!(st.take_read_os() && !st.take_read_os(), "one RequestPaste");
        assert_eq!(st.on_pass(pass(&[], 11)), [Act::Repaint]);
        let ev = [Event::Paste(FIXTURE.into())];
        assert_eq!(
            st.on_pass(pass(&ev, 12)),
            [Act::Paste(json!({ "clip": FIXTURE }))]
        );
        assert_eq!(
            st.on_pass(pass(&ev, 13)),
            [],
            "an unasked native Paste is ignored"
        );

        st.paste_key(true);
        st.on_pass(pass(&[], 20));
        st.on_pass(pass(&[], 22));
        assert_eq!(
            st.on_pass(pass(&[], 23)),
            [Act::Paste(Value::Null)],
            "no text: the session clip"
        );

        st.paste_key(true);
        st.on_pass(pass(&[], 30));
        let img = [Event::PasteImage(Default::default())];
        assert_eq!(
            st.on_pass(pass(&img, 31)),
            [Act::Paste(Value::Null)],
            "an image: the session clip"
        );

        st.paste_key(true);
        st.on_pass(pass(&[], 40));
        let prose = [Event::Paste("some prose".into())];
        assert_eq!(st.on_pass(pass(&prose, 42)), [Act::Paste(Value::Null)]);
    }

    #[test]
    fn text_fields_keep_their_clipboard() {
        let mut st = ClipState::default();
        st.paste_key(true);
        let ev = [Event::Copy, Event::Paste(FIXTURE.into())];
        let p = Pass {
            text_focus: true,
            ..pass(&ev, 1)
        };
        assert_eq!(st.on_pass(p), []);
        assert!(
            st.take_read_os(),
            "the text field still gets the clipboard text"
        );
        assert_eq!(st.on_pass(pass(&[], 5)), [], "nothing waits for it");
        let p = Pass {
            label_selection: true,
            ..pass(&[Event::Copy], 6)
        };
        assert_eq!(st.on_pass(p), [], "egui copies the selected label text");
    }

    #[test]
    fn copy_cut_and_the_paste_command() {
        let mut st = ClipState::default();
        assert_eq!(st.on_pass(pass(&[Event::Copy], 1)), [Act::Copy]);
        assert_eq!(st.on_pass(pass(&[Event::Copy, Event::Cut], 2)), [Act::Cut]);
        let none = Pass {
            has_selection: false,
            ..pass(&[Event::Copy], 3)
        };
        assert_eq!(st.on_pass(none), []);
        // Edit → Paste from the palette: its text field still has focus
        // for a pass, and the paste is the net's regardless.
        st.paste_command(true);
        let focused = Pass {
            text_focus: true,
            ..pass(&[], 4)
        };
        assert_eq!(st.on_pass(focused), [Act::Repaint]);
        let v2 = FIXTURE.replace("\"version\": 1", "\"version\": 2");
        let ev = [Event::Paste(v2)];
        assert!(
            matches!(&st.on_pass(pass(&ev, 5))[..], [Act::Refuse(m)] if m.contains("version 2"))
        );
        // The web: the browser's paste arrives unasked.
        let ev = [Event::Paste(FIXTURE.into())];
        let web = Pass {
            native: false,
            ..pass(&ev, 6)
        };
        assert_eq!(st.on_pass(web), [Act::Paste(json!({ "clip": FIXTURE }))]);
    }

    #[test]
    fn counts_read_naturally() {
        let v = json!({"places": 2, "transitions": 1, "arcs": 0});
        assert_eq!(counts(&v), "2 places, 1 transition and 0 arcs");
    }
}
