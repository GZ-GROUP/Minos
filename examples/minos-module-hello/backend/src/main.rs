//! A module backend: reads one JSON request per line from stdin and writes one JSON
//! response per line to stdout. It can be written in any language. See the protocol
//! in `docs/modules.md`.

use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

fn handle(method: &str, params: &Value) -> Result<Value, (&'static str, String)> {
    match method {
        "greet" => {
            let name = params["name"].as_str().unwrap_or("world");
            Ok(json!({ "greeting": format!("Hello, {name}!") }))
        }
        other => Err(("unknown_method", format!("unknown method \"{other}\""))),
    }
}

fn respond(request: &str) -> Value {
    let request: Value = match serde_json::from_str(request) {
        Ok(request) => request,
        Err(error) => {
            return json!({
                "id": null,
                "error": { "code": "invalid_request", "message": error.to_string() }
            });
        }
    };

    let id = &request["id"];
    let method = request["method"].as_str().unwrap_or_default();
    match handle(method, &request["params"]) {
        Ok(result) => json!({ "id": id, "result": result }),
        Err((code, message)) => json!({ "id": id, "error": { "code": code, "message": message } }),
    }
}

fn main() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let response = respond(&line?);
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_a_known_method() {
        let response = respond(r#"{"id":1,"method":"greet","params":{"name":"Minos"}}"#);
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["greeting"], "Hello, Minos!");
    }

    #[test]
    fn reports_errors_without_stopping() {
        assert_eq!(
            respond(r#"{"id":2,"method":"nope"}"#)["error"]["code"],
            "unknown_method"
        );
        assert_eq!(respond("not json")["error"]["code"], "invalid_request");
    }
}
