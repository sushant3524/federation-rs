use router_bridge::planner::{IncrementalDeliverySupport, Planner, QueryPlannerConfig};

#[tokio::main]
async fn main() {
    for i in 0..100 {
        let schema = format!(
            r#"
                schema
                    @core(feature: "https://specs.apollo.dev/core/v0.1")
                    @core(feature: "https://specs.apollo.dev/join/v0.1")
                    @core(feature: "https://specs.apollo.dev/inaccessible/v0.1")
                    {{
                    query: Query
                }}
                directive @core(feature: String!) repeatable on SCHEMA
                directive @join__field(graph: join__Graph, requires: join__FieldSet, provides: join__FieldSet) on FIELD_DEFINITION
                directive @join__type(graph: join__Graph!, key: join__FieldSet) repeatable on OBJECT | INTERFACE
                directive @join__owner(graph: join__Graph!) on OBJECT | INTERFACE
                directive @join__graph(name: String!, url: String!) on ENUM_VALUE
                directive @inaccessible on OBJECT | FIELD_DEFINITION | INTERFACE | UNION
                scalar join__FieldSet
                enum join__Graph {{
                USER @join__graph(name: "user", url: "http://localhost:4001/graphql")
                ORGA @join__graph(name: "orga", url: "http://localhost:4002/graphql")
                }}
                type Query {{
                currentUser: User{i} @join__field(graph: USER)
                }}
                type User{i}
                @join__owner(graph: USER)
                @join__type(graph: ORGA, key: "id")
                @join__type(graph: USER, key: "id"){{
                id: ID!
                name: String
                activeOrganization: Organization
                }}
                type Organization
                @join__owner(graph: ORGA)
                @join__type(graph: ORGA, key: "id")
                @join__type(graph: USER, key: "id") {{
                id: ID
                creatorUser: User{i}
                name: String
                nonNullId: ID!
                suborga: [Organization]
                }}"#,
        );

        let planner = Planner::<serde_json::Value>::new(
            schema.to_string(),
            QueryPlannerConfig {
                incremental_delivery: Some(IncrementalDeliverySupport {
                    enable_defer: Some(true),
                }),
            },
        )
        .await
        .unwrap();

        let _ = &planner
                .plan(
                    "query { currentUser { activeOrganization { id  suborga { id ...@defer { nonNullId } } } } }"
                    .to_string(),
                    None
                )
                .await
                .unwrap()
                .data
                .unwrap();

        if i % 10 == 0 {
            println!("before");
            planner.dump_resources().await.unwrap();
            planner.gc().await.unwrap();
            println!("after");
            planner.dump_resources().await.unwrap();
        }
    }
}
