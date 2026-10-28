//! `pflow-native`: the desktop app (and, built for wasm32, the web app).
//!
//! ```text
//! pflow-native [NET.json] [--example NAME] [--cmd 'ID {json}']... [--screenshot OUT.png]
//!              [--size WxH] [--control ADDR | --no-control] [--fresh]
//! pflow-native --composition-goldens OUT.json
//! ```
//!
//! `--composition-goldens` draws every composition view × example × state
//! headless (no window) and writes the manifests the drawing collected
//! (`parity/composition-views/goldens.json`).

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use pflow_native::{App, Options};
    use pflow_native_core::Step;

    struct Native;
    impl pflow_native::Services for Native {
        fn pick_open(&self) -> Option<String> {
            rfd::FileDialog::new()
                .add_filter("pflow JSON", &["json", "jsonld"])
                .pick_file()
                .map(|p| p.display().to_string())
        }
        fn pick_open_kind(&self, name: &str, exts: &[&str]) -> Option<String> {
            rfd::FileDialog::new()
                .add_filter(name, exts)
                .pick_file()
                .map(|p| p.display().to_string())
        }
        fn pick_save(&self, default_name: &str, ext: &str) -> Option<String> {
            rfd::FileDialog::new()
                .set_file_name(default_name)
                .add_filter(ext, &[ext])
                .save_file()
                .map(|p| p.display().to_string())
        }
    }

    let mut opts = Options {
        control: Some(pflow_native::control::DEFAULT_ADDR.to_string()),
        ..Options::default()
    };
    let mut size = [1400.0_f32, 900.0];
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--composition-goldens" => {
                let out = args
                    .next()
                    .unwrap_or_else(|| "parity/composition-views/goldens.json".into());
                match pflow_native::compviews::goldens() {
                    Ok(v) => {
                        let text = serde_json::to_string_pretty(&v).expect("JSON") + "\n";
                        if let Err(e) = std::fs::write(&out, text) {
                            eprintln!("{out}: {e}");
                            std::process::exit(1);
                        }
                        println!("wrote {out}");
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("composition goldens: {e}");
                        std::process::exit(1);
                    }
                }
            }
            "--composition-goldens-transition" => {
                let out = args
                    .next()
                    .unwrap_or_else(|| "goldens-transition.json".into());
                match pflow_native::compviews::goldens_transition() {
                    Ok(v) => {
                        let text = serde_json::to_string_pretty(&v).expect("JSON") + "\n";
                        if let Err(e) = std::fs::write(&out, text) {
                            eprintln!("{out}: {e}");
                            std::process::exit(1);
                        }
                        println!("wrote {out}");
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("composition goldens (transition): {e}");
                        std::process::exit(1);
                    }
                }
            }
            "--cmd" => {
                let line = args.next().unwrap_or_default();
                let (id, rest) = line.split_once(char::is_whitespace).unwrap_or((&line, ""));
                let args = if rest.trim().is_empty() {
                    serde_json::Value::Null
                } else {
                    match serde_json::from_str(rest) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("--cmd {id}: args are not JSON: {e}");
                            std::process::exit(2);
                        }
                    }
                };
                opts.commands.push(Step {
                    command: id.to_string(),
                    args,
                });
            }
            "--example" => {
                let name = args.next().unwrap_or_default();
                opts.commands.insert(
                    0,
                    Step {
                        command: "file.open_example".into(),
                        args: serde_json::json!({ "name": name }),
                    },
                );
            }
            "--screenshot" => opts.screenshot = args.next(),
            "--control" => opts.control = args.next(),
            "--no-control" => opts.control = None,
            "--fresh" => opts.fresh = true,
            "--size" => {
                let s = args.next().unwrap_or_default();
                if let Some((w, h)) = s.split_once('x') {
                    size = [w.parse().unwrap_or(size[0]), h.parse().unwrap_or(size[1])];
                }
            }
            "-h" | "--help" => {
                println!(
                    "pflow-native [NET.json] [--example NAME] [--cmd 'ID {{json}}']... \
                     [--screenshot OUT.png] [--size WxH] [--control ADDR | --no-control] [--fresh]\n\n\
                     --control ADDR  JSON-lines control port for agents (default {}; `pflow-native-cli mcp` attaches to it)\n\
                     --fresh         ignore the autosaved document and settings",
                    pflow_native::control::DEFAULT_ADDR
                );
                return Ok(());
            }
            path => opts.open = Some(path.to_string()),
        }
    }
    let native = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_title("pflow-native"),
        // Cmd+C/X/V arrive as keys, so Cmd+V is seen even when the system
        // clipboard holds no text; the app makes the clipboard events
        // itself (crates/app/src/clipboard.rs).
        clipboard_shortcuts: false,
        ..Default::default()
    };
    eframe::run_native(
        "pflow-native",
        native,
        Box::new(move |cc| Ok(Box::new(App::new(opts, Some(Box::new(Native)), cc.storage)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("pflow_native_canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("index.html needs <canvas id=\"pflow_native_canvas\">");
        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| {
                    Ok(Box::new(pflow_native::App::new(
                        Default::default(),
                        None,
                        cc.storage,
                    )))
                }),
            )
            .await
            .expect("failed to start eframe");
    });
}
