use crate::error::Error;
/// Wraps creating the Deno Js runtime collecting parameters and executing a script.
use deno_core::{op_sync, JsRuntime, RuntimeOptions, Snapshot};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::mpsc::channel;

#[derive(Debug, Clone)]
pub(crate) struct Js {
    snapshot: Box<[u8]>,
    parameters: Vec<(&'static str, String)>,
}

impl Js {
    pub(crate) fn new() -> Self {
        // The snapshot is created in our build.rs script and included in our binary image
        let buffer = include_bytes!("../snapshots/query_runtime.snap");

        Self {
            snapshot: buffer.to_vec().into_boxed_slice(),
            parameters: Vec::new(),
        }
    }

    pub(crate) fn apply_script<Ok: DeserializeOwned + 'static>(
        &mut self,
        name: &'static str,
        source: &'static str,
    ) -> Result<Ok, Error> {
        let options = RuntimeOptions {
            will_snapshot: true,
            ..Default::default()
        };
        let mut runtime = JsRuntime::new(options);

        // The runtime automatically contains a Deno.core object with several
        // functions for interacting with it.
        let runtime_str = include_str!("../js-dist/runtime.js");
        runtime
            .execute_script("<init>", &runtime_str)
            .expect("unable to initialize router bridge runtime environment");

        // Load the composition library.
        let bridge_str = include_str!("../bundled/bridge.js");
        runtime
            .execute_script("bridge.js", &bridge_str)
            .expect("unable to evaluate bridge module");

        // We'll use this channel to get the results
        let (tx, rx) = channel();

        let happy_tx = tx.clone();

        runtime.register_op(
            "op_result",
            op_sync(move |_state, value, _buffer: ()| {
                happy_tx.send(Ok(value)).expect("channel must be open");

                Ok(serde_json::json!(null))

                // Don't return anything to JS
            }),
        );
        runtime.sync_ops_cache();
        for parameter in self.parameters.iter() {
            runtime
                .execute_script(format!("<{}>", parameter.0).as_str(), &parameter.1)
                .expect("unable to evaluate service list in JavaScript runtime");
        }

        for parameter in self.parameters.iter() {
            runtime
                .execute_script(format!("<{}>", parameter.0).as_str(), &parameter.1)
                .expect("unable to evaluate service list in JavaScript runtime");
        }

        let _ = runtime.execute_script(name, source).map_err(|e| {
            let message = format!(
                "unable to invoke {} in JavaScript runtime \n error: \n {:?}",
                source, e
            );

            tx.send(Err(Error::DenoRuntime(message)))
                .expect("channel must be open");

            e
        });

        *self = Self {
            snapshot: runtime.snapshot().to_vec().into_boxed_slice(),
            parameters: Vec::new(),
        };

        rx.recv().expect("channel remains open")
    }

    pub(crate) fn with_parameter<T: Serialize>(
        mut self,
        name: &'static str,
        param: T,
    ) -> Result<Self, Error> {
        let serialized = format!(
            "{} = {}",
            name,
            serde_json::to_string(&param).map_err(|error| Error::ParameterSerialization {
                name: name.to_string(),
                message: error.to_string()
            })?
        );
        self.parameters.push((name, serialized));
        Ok(self)
    }

    pub(crate) fn execute<Ok: DeserializeOwned + 'static>(
        &self,
        name: &'static str,
        source: &'static str,
    ) -> Result<Ok, Error> {
        let options = RuntimeOptions {
            startup_snapshot: Some(Snapshot::Boxed(self.snapshot.clone())),
            ..Default::default()
        };
        let mut runtime = JsRuntime::new(options);

        // We'll use this channel to get the results
        let (tx, rx) = channel();

        let happy_tx = tx.clone();

        runtime.register_op(
            "op_result",
            op_sync(move |_state, value, _buffer: ()| {
                happy_tx.send(Ok(value)).expect("channel must be open");

                Ok(serde_json::json!(null))

                // Don't return anything to JS
            }),
        );
        runtime.sync_ops_cache();
        for parameter in self.parameters.iter() {
            runtime
                .execute_script(format!("<{}>", parameter.0).as_str(), &parameter.1)
                .expect("unable to evaluate service list in JavaScript runtime");
        }

        // We are sending the error through the channel already
        let _ = runtime.execute_script(name, source).map_err(|e| {
            let message = format!(
                "unable to invoke {} in JavaScript runtime \n error: \n {:?}",
                source, e
            );

            tx.send(Err(Error::DenoRuntime(message)))
                .expect("channel must be open");

            e
        });

        rx.recv().expect("channel remains open")
    }
}
