use std::process::Stdio;
use std::time::Duration;

use thiserror::Error;
use tokio::{io::AsyncReadExt, io::AsyncWriteExt, process::Command};

#[derive(Debug, Error)]
pub(crate) enum SatelliteError {
    #[error("launch timed out")]
    LaunchTimeOut,
    #[error("launch failed")]
    LaunchFailed,
    #[error("IO error: {0}")]
    IoFailed(std::io::Error),
}

impl From<std::io::Error> for SatelliteError {
    fn from(error: std::io::Error) -> Self {
        SatelliteError::IoFailed(error)
    }
}

// Errors are "equal", from a testing point of view, if they are the same variant
#[cfg(test)]
impl PartialEq for SatelliteError {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// Launch a child process with a timeout.
pub(crate) async fn launch(
    command: &str,
    args: &[&str],
    payload: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, SatelliteError> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // We don't care about stderr, so send it to null
        .stderr(Stdio::null())
        // Kill our child if we timeout
        .kill_on_drop(true)
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    // Send the payload
    stdin.write_all(payload).await?;

    // Dropping stdin starts processing in the spawned command
    drop(stdin);

    // We may timeout
    tokio::select! {
        () = tokio::time::sleep(timeout) => {
            Err(SatelliteError::LaunchTimeOut)
        },
        output_result = child.wait() => {
            let status = output_result?;
            if status.success() {
                let mut data = vec![];
                let _output = stdout.read_to_end(&mut data).await?;
                Ok(data)
            } else {
                Err(SatelliteError::LaunchFailed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_will_timeout_the_launch() {
        assert_eq!(
            launch(
                "node",
                &[],
                b"console.log('hello');\n",
                Duration::from_millis(1)
            )
            .await,
            Err(SatelliteError::LaunchTimeOut)
        );
    }

    #[tokio::test]
    async fn it_will_not_timeout_the_launch() {
        assert_eq!(
            launch(
                "node",
                &[],
                b"console.log('hello');\n",
                Duration::from_secs(1)
            )
            .await,
            Ok(b"hello\n".to_vec())
        );
    }

    #[tokio::test]
    async fn it_will_get_an_error_since_node_cannot_process_this() {
        assert_eq!(
            launch(
                "node",
                &[],
                b"Xconsole.log('hello');\n",
                Duration::from_secs(1)
            )
            .await,
            Err(SatelliteError::LaunchFailed)
        );
    }

    #[tokio::test]
    async fn it_will_get_an_error_from_nodey() {
        assert!(matches!(
            launch(
                "nodey",
                &[],
                b"console.log('hello');\n",
                Duration::from_secs(1)
            )
            .await,
            Err(SatelliteError::IoFailed(..))
        ));
    }

    #[tokio::test]
    async fn it_will_run_gqp() {
        /*
        let args_remote = vec![
            "github:@apollosolutions/generate-query-plan",
            "--graphref",
            "starstuff@current",
            "--operation",
            "starop.graphql",
        ];
        */
        let args = vec![
            "--graphref",
            "starstuff@current",
            "--operation",
            "starop.graphql",
        ];
        launch("query-plan-generator", &args, b"", Duration::from_secs(10))
            .await
            .expect("npx failed");
    }

    #[tokio::test]
    async fn it_will_run_gqp_pretty() {
        let args = vec![
            "--graphref",
            "starstuff@current",
            "--operation",
            "starop.graphql",
            "--pretty",
        ];
        launch("query-plan-generator", &args, b"", Duration::from_secs(10))
            .await
            .expect("npx failed");
    }

    #[tokio::test]
    async fn it_will_run_http_planner() {
        let payload =
            tokio::fs::read("/Users/garypen/dev/router/examples/graphql/supergraph.graphql")
                .await
                .expect("it read schema file");
        let args = vec![
            "/Users/garypen/dev/http_planner/dist/cli.js",
            "plan",
            "query MeQuery { me { id } }",
        ];

        launch("node", &args, &payload, Duration::from_secs(10))
            .await
            .expect("npx failed");
    }
}
