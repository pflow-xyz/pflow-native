//! `pflow-native-cli`: the headless front end. Every subcommand drives the
//! same registry the app's menus do.

mod mcp;

use std::io::{self, BufRead, Read, Write};
use std::process::ExitCode;

use pflow_native_core::commands::command::parse_steps;
use pflow_native_core::Session;
use serde_json::{json, Value};

const USAGE: &str = "\
pflow-native-cli — drive the pflow-native command registry headless

USAGE:
  pflow-native-cli list [PREFIX]                     commands with args schemas (JSON)
  pflow-native-cli exec [-i NET] [-o OUT] ID [ARGS]  run one command; ARGS is a JSON object
  pflow-native-cli batch [-i NET] [-o OUT] STEPS     run [{command,args}] (file or - for stdin) as one undo step
  pflow-native-cli control [-i NET] [-o OUT]         JSON-lines channel on stdin/stdout:
                                                       {\"id\":1,\"command\":\"net.info\",\"args\":{}}
                                                       -> {\"id\":1,\"ok\":true,\"result\":...}
  pflow-native-cli mcp [--attach ADDR | --headless] [-i NET] [-o OUT]
                                                     MCP server on stdio (list_commands, execute, batch,
                                                     get_state, screenshot). Attaches to a running app's
                                                     control port ($PFLOW_NATIVE_ADDR or 127.0.0.1:7979, with
                                                     the token from the app's control.json) when one is up,
                                                     else runs headless. -i alone implies headless; with
                                                     --attach it opens NET in the app.

  pflow-native-cli login sim|pflow [--no-browser]   sign in: sim.pflow.xyz (browser, Google) or pflow.xyz
                                                     (GitHub: enter the printed code); waits until done
  pflow-native-cli logout sim|pflow                  forget that sign-in (auth.logout)
  pflow-native-cli accounts                          what you are signed in to (auth.status)

  -i NET   open this pflow JSON file first (file.open)
  -o OUT   save the document here afterwards (file.save; for control/mcp: when stdin closes)
";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let input = take_flag(&mut args, "-i");
    let attach = take_flag(&mut args, "--attach");
    let headless = take_switch(&mut args, "--headless");
    let output = take_flag(&mut args, "-o");
    let no_browser = take_switch(&mut args, "--no-browser");
    let Some(sub) = (!args.is_empty()).then(|| args.remove(0)) else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let mut sess = Session::default();
    if let Some(path) = &input {
        if let Err(e) = sess.execute("file.open", json!({ "path": path })) {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    }
    let result = match sub.as_str() {
        "list" => sess.execute(
            "command.list",
            args.first().map_or(Value::Null, |p| json!({ "prefix": p })),
        ),
        "exec" => match args.first() {
            None => {
                eprint!("{USAGE}");
                return ExitCode::from(2);
            }
            Some(id) => match args.get(1).map(|a| serde_json::from_str(a)) {
                Some(Err(e)) => {
                    eprintln!("error: ARGS is not JSON: {e}");
                    return ExitCode::from(2);
                }
                parsed => sess.execute(id, parsed.and_then(Result::ok).unwrap_or(Value::Null)),
            },
        },
        "batch" => {
            let src = match args.first().map(String::as_str) {
                None | Some("-") => {
                    let mut s = String::new();
                    io::stdin().read_to_string(&mut s).map(|_| s)
                }
                Some(path) => std::fs::read_to_string(path),
            };
            let steps = src
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| e.to_string()))
                .and_then(|v| parse_steps(&v).map_err(|e| e.to_string()));
            match steps {
                Ok(steps) => sess.batch(&steps),
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(2);
                }
            }
        }
        "control" => {
            let code = control(&mut sess);
            return save_on_exit(code, output.as_deref(), |args| {
                sess.execute("file.save", args).map_err(|e| e.to_string())
            });
        }
        "mcp" => {
            let addr = attach
                .clone()
                .or_else(|| std::env::var("PFLOW_NATIVE_ADDR").ok())
                .unwrap_or_else(|| "127.0.0.1:7979".into());
            let backend = if headless || (input.is_some() && attach.is_none()) {
                None
            } else {
                match mcp::Backend::connect(&addr) {
                    Ok(b) => Some(b),
                    Err(e) if attach.is_some() => {
                        eprintln!("error: cannot attach to {addr}: {e}");
                        return ExitCode::FAILURE;
                    }
                    Err(_) => None,
                }
            };
            let mut be = backend.unwrap_or(mcp::Backend::Headless(Box::new(sess)));
            // `-i NET` with `--attach`: open it in the app (one undo step
            // there), instead of silently dropping it.
            if let (Some(path), Some(_)) = (&input, be.attached()) {
                let abs = std::fs::canonicalize(path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| path.clone());
                if let Err(e) = be.exec("file.open", json!({ "path": abs })) {
                    eprintln!("error: opening {path} in the app: {e}");
                    return ExitCode::FAILURE;
                }
            }
            let mut be = mcp::serve(be);
            return save_on_exit(ExitCode::SUCCESS, output.as_deref(), |args| {
                be.exec("file.save", args)
            });
        }
        "login" => return login(&mut sess, args.first().map(String::as_str), no_browser),
        "logout" => sess.execute("auth.logout", json!({ "service": args.first() })),
        "accounts" => sess.execute("auth.status", Value::Null),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("unknown subcommand {other:?}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(v) => {
            match v {
                Value::String(s) => println!("{s}"),
                v => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
            }
            if let Some(path) = output {
                if let Err(e) = sess.execute("file.save", json!({ "path": path })) {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `-o OUT` for the long-running subcommands: save when stdin closes.
fn save_on_exit(
    code: ExitCode,
    output: Option<&str>,
    mut save: impl FnMut(Value) -> Result<Value, String>,
) -> ExitCode {
    let Some(path) = output else { return code };
    let abs = std::path::absolute(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.to_string());
    match save(json!({ "path": abs })) {
        Ok(_) => code,
        Err(e) => {
            eprintln!("error: saving {path}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn take_switch(args: &mut Vec<String>, flag: &str) -> bool {
    match args.iter().position(|a| a == flag) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    }
}

/// `login SERVICE`: starts the sign-in, says on stderr what to do in the
/// browser, and waits for it.
fn login(sess: &mut Session, service: Option<&str>, no_browser: bool) -> ExitCode {
    use pflow_native_core::auth;
    let started = auth::Service::parse(service.unwrap_or("")).and_then(|svc| {
        sess.execute(
            "auth.login",
            json!({ "service": svc.name(), "browser": !no_browser }),
        )
        .map(|info| (svc, info))
    });
    let (svc, info) = match started {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(note) = info["note"].as_str() {
        eprintln!("{note}");
    }
    if no_browser || info.get("browser").is_some() {
        eprintln!("Open: {}", info["open"].as_str().unwrap_or(""));
    }
    eprintln!("Waiting…");
    match auth::wait(svc, std::time::Duration::from_secs(900)) {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
}

/// One request per line: `{"id", "command", "args"}` or `{"id", "batch": [...]}`.
fn control(sess: &mut Session) -> ExitCode {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(e) => json!({ "id": null, "ok": false, "error": format!("bad JSON: {e}") }),
            Ok(req) => {
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let r = if let Some(steps) = req.get("batch") {
                    parse_steps(steps).and_then(|s| sess.batch(&s))
                } else if let Some(cmd) = req.get("command").and_then(Value::as_str) {
                    sess.execute(cmd, req.get("args").cloned().unwrap_or(Value::Null))
                } else {
                    Err(pflow_native_core::Error::BadArgs(
                        "expected \"command\" or \"batch\"".into(),
                    ))
                };
                match r {
                    Ok(v) => json!({ "id": id, "ok": true, "result": v }),
                    Err(e) => json!({ "id": id, "ok": false, "error": e.to_string() }),
                }
            }
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
    ExitCode::SUCCESS
}
