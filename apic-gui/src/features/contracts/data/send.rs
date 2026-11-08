//! Sending a contract's request on a background thread, so the window keeps
//! drawing while the server answers. No egui: the caller polls the channel.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use apic_core::http::{self, Reply};
use apic_core::json::JsonContent;
use apic_core::vars::Vars;

/// How long a send may take, from connecting to the end of the body.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Sends `contract` filled from `vars` on a new thread, giving up after 30 s.
///
/// # Arguments
///
/// * `contract` - the request to send
/// * `vars` - the variable values to fill it with
/// * `project_root` - the directory file parts resolve under
///
/// # Returns
///
/// The channel the reply or the error arrives on, once. Dropping it discards
/// the result when it arrives.
pub(crate) fn start(
    contract: JsonContent,
    vars: Vars,
    project_root: PathBuf,
) -> Receiver<Result<Reply, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = http::send(&contract, &vars, &project_root, TIMEOUT);
        // A cancelled send dropped the receiver, so nobody is left to tell.
        let _ = tx.send(result);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::{serve_once, tempdir};

    #[test]
    fn start_delivers_the_reply_on_its_channel() {
        let (url, _request) = serve_once("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
        let contract = apic_core::json::json_get(
            &format!(
                r#"{{ "name": "x", "method": "GET", "url": "{url}/ping", "headers": [], "responses": [] }}"#
            ),
            None,
        )
        .expect("contract parses");

        let rx = start(contract, Vars::new(), tempdir());

        let reply = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the job answers")
            .expect("the send succeeds");
        assert_eq!(reply.status, 204);
    }
}
