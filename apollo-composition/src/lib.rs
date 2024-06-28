use apollo_compiler::Schema;
use apollo_federation::sources::connect::expand::{expand_connectors, Connectors, ExpansionResult};
use apollo_federation::sources::connect::{validate, Location, ValidationCode};
use either::Either;
use std::iter::once;

use apollo_federation_types::build_plugin::{
    BuildMessage, BuildMessageLevel, BuildMessageLocation, BuildMessagePoint,
};
use apollo_federation_types::javascript::{
    CompositionHint, GraphQLError, SatisfiabilityResult, SubgraphASTNode, SubgraphDefinition,
};

/// This trait includes all the Rust-side composition logic, plus hooks for the JavaScript side.
/// If you implement the functions in this trait to build your own JavaScript interface, then you
/// can call [`HybridComposition::compose`] to run the complete composition process.
///
/// JavaScript should be implemented using `@apollo/composition@2.9.0-connectors.0`.
#[allow(async_fn_in_trait)]
pub trait HybridComposition {
    /// Call the JavaScript `composeServices` function from `@apollo/composition` plus whatever
    /// extra logic you need. Make sure to disable satisfiability, like `composeServices(definitions, {runSatisfiability: false})`
    async fn compose_services_without_satisfiability(
        &mut self,
        subgraph_definitions: Vec<SubgraphDefinition>,
    ) -> Option<SupergraphSdl>;

    /// Call the JavaScript `validateSatisfiability` function from `@apollo/composition` plus whatever
    /// extra logic you need.
    ///
    /// # Input
    ///
    /// The `validateSatisfiability` function wants an argument like `{ supergraphSdl }`. That field
    /// should be the value that's updated when [`update_supergraph_sdl`] is called.
    ///
    /// # Output
    ///
    /// If satisfiability completes from JavaScript, the [`SatisfiabilityResult`] (matching the shape
    /// of that function) should be returned. If Satisfiability _can't_ be run, you can return an
    /// `Err(Issue)` instead indicating what went wrong.
    async fn validate_satisfiability(&mut self) -> Result<SatisfiabilityResult, Issue>;

    /// Allows the Rust composition code to modify the stored supergraph SDL
    /// (for example, to expand connectors).
    fn update_supergraph_sdl(&mut self, supergraph_sdl: String);

    /// When the Rust composition/validation code finds issues, it will call this method to add
    /// them to the list of issues that will be returned to the user.
    ///
    /// It's on the implementor of this trait to convert `From<Issue>`
    fn add_issues<Source: Iterator<Item = Issue>>(&mut self, issues: Source);

    /// Runs the complete composition process, hooking into both the Rust and JavaScript implementations.
    ///
    /// # Asyncness
    ///
    /// While this function is async to allow for flexible JavaScript execution, it is a CPU-heavy task.
    /// Take care when consuming this in an async context, as it may block longer than desired.
    ///
    /// # Algorithm
    ///
    /// 1. Run Rust-based validation on the subgraphs
    /// 2. Call [`compose_services_without_satisfiability`] to run JavaScript-based composition
    /// 3. Run Rust-based validation on the supergraph
    /// 4. Call [`validate_satisfiability`] to run JavaScript-based validation on the supergraph
    async fn compose(&mut self, subgraph_definitions: Vec<SubgraphDefinition>) {
        let subgraph_validation_errors = subgraph_definitions
            .iter()
            .flat_map(|subgraph| {
                // TODO: Use parse_and_validate (adding in directives as needed)
                // TODO: Handle schema errors rather than relying on JavaScript to catch it later
                let schema = Schema::parse(&subgraph.sdl, &subgraph.name)
                    .unwrap_or_else(|schema_with_errors| schema_with_errors.partial);
                validate(schema).into_iter().map(|validation_error| Issue {
                    code: transform_code(validation_error.code),
                    message: validation_error.message,
                    locations: validation_error
                        .locations
                        .into_iter()
                        .map(|locations| SubgraphLocation {
                            subgraph: subgraph.name.clone(),
                            start: locations.start,
                            end: locations.end,
                        })
                        .collect(),
                    severity: severity(validation_error.code),
                })
            })
            .collect::<Vec<_>>();
        if !subgraph_validation_errors.is_empty() {
            self.add_issues(subgraph_validation_errors.into_iter());
            return;
        }

        let Some(supergraph_sdl) = self
            .compose_services_without_satisfiability(subgraph_definitions)
            .await
        else {
            return;
        };

        let expansion_result = match expand_connectors(supergraph_sdl) {
            Ok(result) => result,
            Err(err) => {
                self.add_issues(once(Issue {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!(
                        "Composition failed due to an internal error, please report this: {}",
                        err
                    ),
                    locations: vec![],
                    severity: Severity::Error,
                }));
                return;
            }
        };
        match expansion_result {
            ExpansionResult::Expanded {
                raw_sdl,
                connectors: Connectors {
                    by_service_name, ..
                },
                ..
            } => {
                self.update_supergraph_sdl(raw_sdl);
                let satisfiability_result = self.validate_satisfiability().await;
                self.add_issues(
                    satisfiability_result_into_issues(satisfiability_result)
                        .map(|mut issue| {
                            for (service_name, connector) in by_service_name.iter() {
                                issue.message = issue.message.replace(
                                    service_name.as_str(),
                                    connector.id.subgraph_name.as_str(),
                                );
                            }
                            issue
                        })
                        .chain(once(Issue {
                            code: "EXPERIMENTAL_FEATURE".to_string(),
                            message: "Connectors are an experimental feature. Breaking changes are likely to occur in future versions.".to_string(),
                            locations: vec![],
                            severity: Severity::Warning,
                        })),
                );
            }
            ExpansionResult::Unchanged => {
                let satisfiability_result = self.validate_satisfiability().await;
                self.add_issues(satisfiability_result_into_issues(satisfiability_result));
            }
        }
    }
}

pub type SupergraphSdl<'a> = &'a str;

/// A successfully composed supergraph, optionally with some issues that should be addressed.
#[derive(Clone, Debug)]
pub struct PartialSuccess {
    pub supergraph_sdl: String,
    pub issues: Vec<Issue>,
}

/// Some issue the user should address. Errors block composition, warnings do not.
#[derive(Clone, Debug)]
pub struct Issue {
    pub code: String,
    pub message: String,
    pub locations: Vec<SubgraphLocation>,
    pub severity: Severity,
}

/// A location in a subgraph's SDL
#[derive(Clone, Debug)]
pub struct SubgraphLocation {
    pub subgraph: String,
    pub start: Location,
    pub end: Location,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

fn transform_code(code: ValidationCode) -> String {
    match code {
        ValidationCode::GraphQLError => "GRAPHQL_ERROR",
        ValidationCode::DuplicateSourceName => "DUPLICATE_SOURCE_NAME",
        ValidationCode::InvalidSourceName => "INVALID_SOURCE_NAME",
        ValidationCode::EmptySourceName => "EMPTY_SOURCE_NAME",
        ValidationCode::SourceScheme => "SOURCE_SCHEME",
        ValidationCode::SourceNameMismatch => "SOURCE_NAME_MISMATCH",
        ValidationCode::SubscriptionInConnectors => "SUBSCRIPTION_IN_CONNECTORS",
        ValidationCode::InvalidUrl => "INVALID_URL",
        ValidationCode::QueryFieldMissingConnect => "QUERY_FIELD_MISSING_CONNECT",
        ValidationCode::AbsoluteConnectUrlWithSource => "ABSOLUTE_CONNECT_URL_WITH_SOURCE",
        ValidationCode::RelativeConnectUrlWithoutSource => "RELATIVE_CONNECT_URL_WITHOUT_SOURCE",
        ValidationCode::NoSourcesDefined => "NO_SOURCES_DEFINED",
        ValidationCode::NoSourceImport => "NO_SOURCE_IMPORT",
        ValidationCode::MultipleHttpMethods => "MULTIPLE_HTTP_METHODS",
        ValidationCode::MissingHttpMethod => "MISSING_HTTP_METHOD",
        ValidationCode::EntityNotOnRootQuery => "ENTITY_NOT_ON_ROOT_QUERY",
        ValidationCode::EntityTypeInvalid => "ENTITY_TYPE_INVALID",
    }
    .to_string()
}

const fn severity(code: ValidationCode) -> Severity {
    // TODO: export this from apollo-federation instead
    match code {
        ValidationCode::NoSourceImport => Severity::Warning,
        _ => Severity::Error,
    }
}

impl From<Severity> for BuildMessageLevel {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Error => BuildMessageLevel::Error,
            Severity::Warning => BuildMessageLevel::Warn,
        }
    }
}

impl From<Issue> for BuildMessage {
    fn from(issue: Issue) -> Self {
        BuildMessage {
            level: issue.severity.into(),
            message: issue.message,
            code: Some(issue.code.to_string()),
            locations: issue
                .locations
                .into_iter()
                .map(|location| location.into())
                .collect(),
            schema_coordinate: None,
            step: None,
            other: Default::default(),
        }
    }
}

impl From<SubgraphLocation> for BuildMessageLocation {
    fn from(location: SubgraphLocation) -> Self {
        BuildMessageLocation {
            subgraph: Some(location.subgraph),
            start: Some(BuildMessagePoint {
                line: Some(location.start.line + 1),
                column: Some(location.start.column + 1),
                start: None,
                end: None,
            }),
            end: Some(BuildMessagePoint {
                line: Some(location.end.line + 1),
                column: Some(location.end.column + 1),
                start: None,
                end: None,
            }),
            source: None,
            other: Default::default(),
        }
    }
}

impl SubgraphLocation {
    fn from_ast(node: SubgraphASTNode) -> Option<Self> {
        Some(Self {
            subgraph: node.subgraph.unwrap_or_default(),
            start: Location {
                line: node.loc.start_token.line? - 1,
                column: node.loc.start_token.column? - 1,
            },
            end: Location {
                line: node.loc.end_token.line? - 1,
                column: node.loc.end_token.column? - 1,
            },
        })
    }
}

impl From<GraphQLError> for Issue {
    fn from(error: GraphQLError) -> Issue {
        Issue {
            code: error
                .extensions
                .map(|extension| extension.code)
                .unwrap_or_default(),
            message: error.message,
            severity: Severity::Error,
            locations: error
                .nodes
                .into_iter()
                .filter_map(SubgraphLocation::from_ast)
                .collect(),
        }
    }
}

impl From<CompositionHint> for Issue {
    fn from(hint: CompositionHint) -> Issue {
        Issue {
            code: hint.definition.code,
            message: hint.message,
            severity: Severity::Warning,
            locations: hint
                .nodes
                .into_iter()
                .filter_map(SubgraphLocation::from_ast)
                .collect(),
        }
    }
}

fn satisfiability_result_into_issues(
    satisfiability_result: Result<SatisfiabilityResult, Issue>,
) -> Either<impl Iterator<Item = Issue>, impl Iterator<Item = Issue>> {
    match satisfiability_result {
        Ok(satisfiability_result) => Either::Left(
            satisfiability_result
                .errors
                .into_iter()
                .flatten()
                .map(Issue::from)
                .chain(
                    satisfiability_result
                        .hints
                        .into_iter()
                        .flatten()
                        .map(Issue::from),
                ),
        ),
        Err(issue) => Either::Right(once(issue)),
    }
}
