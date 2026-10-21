//! Signing in to sim.pflow.xyz (Google, through sim's OAuth server) and
//! pflow.xyz (GitHub device flow). The flows live in `crate::auth`; these
//! commands start, show and end them. None returns a token.

use serde_json::Value;

use crate::auth::{self, LoginOpts, Service};
use crate::registry::{always, arg, opt, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

const SERVICE_HELP: &str =
    "\"sim\" (sim.pflow.xyz, Google sign-in) or \"pflow\" (pflow.xyz, GitHub)";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "auth.login",
        label: "Sign In…",
        help: "Sign in to sim.pflow.xyz (opens the browser on sim's sign-in page, which uses Google) or \
               pflow.xyz (GitHub device flow: enter the returned user_code at the returned URL). Returns \
               at once; the sign-in finishes in the background (auth.status). The token is stored in \
               <data_dir>/credentials.json, owner-only, and is never returned.",
        menu: "",
        shortcut: None,
        args: &[
            arg("service", Str, SERVICE_HELP),
            opt("server", Str, "sim only: the sim server (default https://sim.pflow.xyz)"),
            opt("scope", Str, "space-separated scopes (default: all sim scopes / read:user user:email gist)"),
            opt("browser", Bool, "open the system browser (default true)"),
        ],
        enabled: always,
        run: login,
        journal: false,
    },
    CommandSpec {
        id: "auth.status",
        label: "Accounts",
        help: "Which services you are signed in to, how (sign-in, environment or credentials.json), \
               token expiry, and any sign-in in progress (with its link and code). Never token values.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: status,
        journal: false,
    },
    CommandSpec {
        id: "auth.logout",
        label: "Sign Out…",
        help: "Delete a service's stored sign-in from credentials.json (and stop one in progress). \
               Tokens set in the environment are left alone; the result lists them.",
        menu: "",
        shortcut: None,
        args: &[arg("service", Str, SERVICE_HELP)],
        enabled: always,
        run: logout,
        journal: false,
    },
];

fn service(a: &Value) -> Result<Service> {
    Service::parse(
        a.get("service")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::BadArgs(format!("service: {SERVICE_HELP}")))?,
    )
}

fn login(_: &mut Session, a: &Value) -> Result<Value> {
    let s = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_string);
    auth::login(
        service(a)?,
        &LoginOpts {
            server: s("server"),
            scope: s("scope"),
            browser: a.get("browser").and_then(Value::as_bool).unwrap_or(true),
        },
    )
}

fn status(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(auth::status())
}

fn logout(_: &mut Session, a: &Value) -> Result<Value> {
    auth::logout(service(a)?)
}
