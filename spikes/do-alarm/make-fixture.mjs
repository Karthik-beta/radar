// Writes fixture.json (a GraphQL search response) and expected.json (its counts).
// Usage: node make-fixture.mjs [--prs 20] [--check]
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const here = (name) => fileURLToPath(new URL(name, import.meta.url));

const pr = (i) => ({
  id: `PR_kwDOA${String(i).padStart(8, '0')}`,
  number: 4000 + i,
  title: `Support the D1 batch API in the migrator, part ${i} of a longer series of changes`,
  url: `https://github.com/drizzle-team/drizzle-orm/pull/${4000 + i}`,
  isDraft: i % 7 === 0,
  state: 'OPEN',
  reviewDecision: i % 3 === 0 ? 'CHANGES_REQUESTED' : null,
  createdAt: '2026-10-20T09:14:02Z',
  updatedAt: '2026-11-04T17:40:11Z',
  mergedAt: null,
  closedAt: null,
  author: { login: 'karthik', __typename: 'User' },
  repository: { nameWithOwner: 'drizzle-team/drizzle-orm' },
  reviews: { nodes: Array.from({ length: 5 }, (_, j) => ({ author: { login: `maint${j}`, __typename: 'User' }, state: j % 2 ? 'COMMENTED' : 'CHANGES_REQUESTED', submittedAt: '2026-11-02T08:00:00Z' })) },
  commits: { nodes: [{ commit: { statusCheckRollup: { state: i % 4 === 0 ? 'FAILURE' : 'SUCCESS' } } }] },
  timelineItems: { nodes: Array.from({ length: 10 }, (_, j) => ({ __typename: j % 3 ? 'IssueComment' : 'PullRequestReview', createdAt: '2026-11-03T10:00:00Z', actor: null, author: { login: j % 4 ? 'karthik' : 'codecov', __typename: j % 4 ? 'User' : 'Bot' } })) },
});

export const generate = (n) => {
  const nodes = Array.from({ length: n }, (_, i) => pr(i));
  return {
    fixture: JSON.stringify({ data: { search: { nodes } } }),
    expected: `${JSON.stringify({
      prs: nodes.length,
      failing: nodes.filter((p) => p.commits.nodes.some((c) => c.commit.statusCheckRollup?.state === 'FAILURE')).length,
      drafts: nodes.filter((p) => p.isDraft).length,
    })}\n`,
  };
};

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { prs: { type: 'string', default: '20' }, check: { type: 'boolean', default: false } } });
  const n = Number(values.prs);
  if (!Number.isInteger(n) || n < 1) {
    console.error('--prs must be a positive integer');
    process.exit(2);
  }
  const { fixture, expected } = generate(n);
  if (values.check) {
    const fixtureOk = readFileSync(here('fixture.json'), 'utf8') === fixture;
    const expectedOk = readFileSync(here('expected.json'), 'utf8') === expected;
    console.log(`fixture.json ${fixtureOk ? 'matches' : 'differs from'} the ${n}-PR generator`);
    console.log(`expected.json ${expectedOk ? 'matches' : 'differs from'} the ${n}-PR generator: ${expected.trim()}`);
    process.exit(fixtureOk && expectedOk ? 0 : 1);
  }
  writeFileSync(here('fixture.json'), fixture);
  writeFileSync(here('expected.json'), expected);
  console.log(`wrote fixture.json (${n} PRs, ${(fixture.length / 1024).toFixed(0)} KB) and expected.json ${expected.trim()}`);
}
