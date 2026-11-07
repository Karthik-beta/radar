# GitHub stub

`server.mjs` plays GitHub for local Radar runs and live lanes. It is Node with no dependencies. It binds `127.0.0.1` only, and it serves a scenario from `scenarios/<name>.json`.

```
node tools/github-stub/server.mjs --port 8900 --scenario basic
stub listening on 127.0.0.1:8900 scenario=basic
```

Point Radar at it with `--var GITHUB_API_BASE:http://127.0.0.1:8900 --var GITHUB_WEB_BASE:http://127.0.0.1:8900`.

## Flags

| Flag | Default | Meaning |
| --- | --- | --- |
| `--port` | `8900` | Port to listen on. `0` picks a free port and prints it. |
| `--scenario` | `basic` | Scenario file name without `.json`. The process exits 1 when it cannot load. |

## Endpoints

| Endpoint | Auth | Behavior |
| --- | --- | --- |
| `GET /user` | token | The scenario `user`. |
| `GET /rate_limit` | token | `{resources: {core, graphql, search}, rate}` built from the scenario `rate`. |
| `GET /notifications` | token | The scenario `notifications.items`. Honors `If-None-Match` and `If-Modified-Since` with 304. Sends `Last-Modified`, `ETag`, and `X-Poll-Interval`. |
| `POST /graphql` | token | Reads `{query, variables}`, finds the operation name, and answers from `graphql[<name>]`. |
| `GET /login/oauth/authorize` | none | Redirects to `redirect_uri` with `code` and the same `state`, or with `error=access_denied` when `oauth.deny` is true. |
| `POST /login/oauth/access_token` | none | Takes a form or JSON body. Checks `client_id`, `client_secret`, and `code` against `oauth`. Answers `bad_verification_code` otherwise. |
| `GET /__log` | none | Every request so far, as a JSON array. |
| `POST /__log/clear` | none | Empties the log. |
| `POST /__scenario` | none | Body `{"name": "<scenario>"}` swaps the scenario and re-arms its faults. 404 for an unknown name. |

The token is the scenario `token` (`gho_stubtoken0000`), sent as `Authorization: Bearer <token>` or `token <token>`. A missing or wrong token gets 401.

Every API response carries `x-ratelimit-limit`, `-remaining`, `-reset` (epoch seconds), `-used`, `-resource`, and `X-OAuth-Scopes` (omitted when `scopes` is not a list).

`/graphql` accepts only `query` documents. The stub reads comments and strings, so a field named `mutationCount` passes. A `mutation`, a `subscription`, a mixed document, or a document it cannot read gets 400. For `graphql[<name>]`, a value with `$pages` answers by the `after` variable, with the key `""` as the first page. A name with no entry answers a GraphQL `errors` body.

## Read-only defence

The stub refuses anything Radar must never send. A method other than GET, or other than POST on `/graphql` and the token endpoint, gets 405. A GraphQL document that is not a query gets 400. Both set `violation` in the log. Unknown paths with GET get 404 and no violation.

## Scenario format

```json
{
  "extends": "basic",
  "token": "gho_stubtoken0000",
  "scopes": ["notifications", "read:user"],
  "user": { "login": "karthik", "id": 1001 },
  "rate": { "limit": 5000, "remaining": 4990, "reset": "@+50m" },
  "poll_interval": 60,
  "notifications": { "last_modified": "@-2m", "etag": "\"stub-etag-1\"", "items": [] },
  "graphql": { "Viewer": { "data": { "viewer": { "login": "karthik" } } } },
  "oauth": { "client_id": "stub-client", "client_secret": "stub-secret", "issue_code": "stub-code", "code": "stub-code", "deny": false },
  "faults": []
}
```

`extends` names a parent scenario. Objects merge key by key, and arrays and scalars replace.

**Relative times.** A string of the form `@now`, `@-2h`, `@+30s`, `@-3d` is replaced with an ISO 8601 timestamp relative to the moment the scenario loads. Units are `s`, `m`, `h`, `d`, and `w`. `rate.reset` and `notifications.last_modified` use this form.

**`rate_limit_poll_interval`.** When true, `GET /rate_limit` also sends `X-Poll-Interval`. GitHub sends it only on `/notifications`, but the R08 placeholder poll reads `/rate_limit` until R09 polls notifications.

**Faults.** Each entry in `faults` matches a request and replaces its response.

| Field | Meaning |
| --- | --- |
| `match` | `"METHOD /path"`. Either part may be `*`. |
| `status` | Response status. Default 500. |
| `times` | Number of requests to fail. Omit it to fail every matching request. |
| `retry_after` | Seconds, sent as `Retry-After`. |
| `rate` | Overrides of `limit`, `remaining`, and `reset` for the rate headers. |
| `body` | JSON body. Default `{"message": "<status text>"}`. |
| `raw` | A string sent as the body as it is, for a response that is not JSON. |

The first entry that still has requests left and matches wins. Faults run after the auth check, so a request with a bad token is a 401.

## Scenarios

| Name | What it changes from `basic` |
| --- | --- |
| `basic` | User `karthik` (id 1001), two notification threads, `Viewer` query, OAuth accepted. |
| `empty` | No notification threads. |
| `fast-poll` | `X-Poll-Interval` of 5, sent on `GET /rate_limit` too, so the R08 placeholder poll sees it. |
| `low-budget` | 400 of 5000 requests left, reset 20 s away. |
| `rate-limited` | `GET /rate_limit` fails four times in order: 403 with `Retry-After`, 429 with `Retry-After`, 403 with zero remaining and a reset only, and a plain 403. |
| `rate-limit-flaky` | `fast-poll`, and `GET /rate_limit` answers 500 three times, then succeeds. |
| `retry-after-once` | `fast-poll`, and the first `GET /rate_limit` answers 403 with `Retry-After: 15`. |
| `server-error` | The four API endpoints answer 500. OAuth still works. |
| `bad-body` | The four API endpoints answer 200 with a body that is not JSON. |
| `graphql-errors` | `Viewer` answers a GraphQL `errors` body. |
| `no-notifications-scope` | Scopes are `read:user` only and `/notifications` answers 404. |
| `oauth-other-user` | `/user` is `someone-else` (id 2002). |
| `oauth-deny` | The authorize endpoint redirects with `error=access_denied`. |
| `oauth-bad-code` | The authorize endpoint issues a code the token endpoint refuses. |

## Log fields

`GET /__log` returns one object per request, in order.

| Field | Meaning |
| --- | --- |
| `n` | Request number, from 1. |
| `method`, `path`, `query` | The request line. `query` has no leading `?`. |
| `ifModifiedSince`, `ifNoneMatch` | The conditional headers, or null. |
| `hasAuth` | Whether an `Authorization` header was present. The value is never logged. |
| `status` | The response status. |
| `violation` | Null, or `method_not_allowed`, `graphql_mutation`, `graphql_subscription`, or `graphql_invalid_document`. |
| `op`, `opName` | GraphQL operation kind and name, on `POST /graphql`. |
| `body`, `variables` | The GraphQL request body and variables, on `POST /graphql` only. |
| `oauth` | `accepted` or `rejected`, on the token endpoint. The code and secret are never logged. |

## Tests

`node --test "tools/**/*.test.mjs"`
