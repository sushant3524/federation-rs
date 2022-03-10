import { ExecutionResult, parse, validate } from "graphql";
import { QueryPlanner, QueryPlan } from "@apollo/query-planner";

import {
  buildSchema,
  operationFromDocument,
} from "@apollo/federation-internals";

export function plan(
  schemaString: string,
  operationString: string,
  operationName?: string
): ExecutionResult<QueryPlan> {
  try {
    const composedSchema = buildSchema(schemaString);
    const apiSchema = composedSchema.toAPISchema();
    const operationDocument = parse(operationString);
    const graphqlJsSchema = apiSchema.toGraphQLJSSchema();

    // Federation does some validation, but not all.  We need to do
    // all default validations that are provided by GraphQL.
    const validationErrors = validate(graphqlJsSchema, operationDocument);
    if (validationErrors.length > 0) {
      return { errors: validationErrors };
    }

    const operation = operationFromDocument(
      composedSchema,
      operationDocument,
      operationName
    );

    const planner = new QueryPlanner(composedSchema);
    return { data: planner.buildQueryPlan(operation) };
  } catch (e) {
    return { errors: [e] };
  }
}

import { GraphQLSchema } from "graphql";

import { Schema } from "@apollo/federation-internals";

let SCHEMA: GraphQLSchema;
let COMPOSED_SCHEMA: Schema;
let PLANNER: QueryPlanner;

export function create(schemaString: string) {
  COMPOSED_SCHEMA = buildSchema(schemaString);
  const apiSchema = COMPOSED_SCHEMA.toAPISchema();
  SCHEMA = apiSchema.toGraphQLJSSchema();
  PLANNER = new QueryPlanner(COMPOSED_SCHEMA);
}

export function plan2(
  operationString: string,
  operationName?: string
): ExecutionResult<QueryPlan> {
  try {
    const operationDocument = parse(operationString);

    // Federation does some validation, but not all.  We need to do
    // all default validations that are provided by GraphQL.
    const validationErrors = validate(SCHEMA, operationDocument);
    if (validationErrors.length > 0) {
      return { errors: validationErrors };
    }

    const operation = operationFromDocument(
      COMPOSED_SCHEMA,
      operationDocument,
      operationName
    );

    return { data: PLANNER.buildQueryPlan(operation) };
  } catch (e) {
    return { errors: [e] };
  }
}
