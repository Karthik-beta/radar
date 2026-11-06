#!/usr/bin/env node
// Marks every changed file in a pull request as Viewed for the signed-in gh user.
// This is developer tooling run by PR owners. Radar itself never writes to GitHub.
// Usage: node scripts/pr-mark-viewed.mjs <pr-number> [owner/repo]
import { execFileSync } from 'node:child_process';

const [number, repoArg] = process.argv.slice(2);
if (!/^\d+$/.test(number ?? '')) {
  console.error('usage: node scripts/pr-mark-viewed.mjs <pr-number> [owner/repo]');
  process.exit(1);
}
const repo = repoArg ?? execFileSync('gh', ['repo', 'view', '--json', 'nameWithOwner', '-q', '.nameWithOwner'], { encoding: 'utf8' }).trim();
const [owner, name] = repo.split('/');

const gql = (query, vars) => {
  const args = ['api', 'graphql', '-f', `query=${query}`];
  for (const [k, v] of Object.entries(vars)) args.push(typeof v === 'number' ? '-F' : '-f', `${k}=${v}`);
  return JSON.parse(execFileSync('gh', args, { encoding: 'utf8' })).data;
};

const FILES = `query($owner: String!, $name: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      id
      files(first: 100, after: $after) { nodes { path viewerViewedState } pageInfo { hasNextPage endCursor } }
    }
  }
}`;

function files() {
  let after = null;
  let id = null;
  const all = [];
  do {
    const vars = { owner, name, number: Number(number) };
    if (after) vars.after = after;
    const pr = gql(FILES, vars).repository.pullRequest;
    id = pr.id;
    all.push(...pr.files.nodes);
    after = pr.files.pageInfo.hasNextPage ? pr.files.pageInfo.endCursor : null;
  } while (after);
  return { id, all };
}

const { id, all } = files();
for (const f of all.filter((f) => f.viewerViewedState !== 'VIEWED')) {
  gql('mutation($id: ID!, $path: String!) { markFileAsViewed(input: { pullRequestId: $id, path: $path }) { clientMutationId } }', { id, path: f.path });
}

const after = files().all;
for (const f of after) console.log(`${f.viewerViewedState === 'VIEWED' ? 'viewed  ' : 'NOT VIEWED'}  ${f.path}`);
const missing = after.filter((f) => f.viewerViewedState !== 'VIEWED');
if (missing.length) {
  console.error(`${missing.length} of ${after.length} files are not marked as viewed`);
  process.exit(1);
}
console.log(`all ${after.length} files in ${repo}#${number} are marked as viewed`);
