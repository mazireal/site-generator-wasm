// Extra constancy-cache cases for the tuned WASM entry: the patterns where
// pre-rendering a constant template could go wrong. Compares our solve.js
// against the baseline makefun-site on each case, the way stress/run.mjs
// does. Usage: node constancy.mjs — exits 1 when any case differs.
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const KIT = '/home/hatch/workspace/makefun/site-generator-speed-01-run-20261122-kit';
const ours = await import(join(HERE, '..', 'solve.js'));
const base = await import(join(KIT, 'baselines', 'makefun-site', 'solve.js'));

const eq = (a, b) => {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) {
    if (a.length !== b.length) return false;
    return a.every((x, i) => eq(x, b[i]));
  }
  const ka = Object.keys(a), kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => Object.hasOwn(b, k) && eq(a[k], b[k]));
};
let pass = 0;
let fail = 0;
function check(label, input) {
  let o, b;
  try {
    o = ours.run(structuredClone(input));
  } catch (e) {
    console.log(`FAIL ${label}: ours threw ${e.message}`);
    fail++;
    return;
  }
  try {
    b = base.run(structuredClone(input));
  } catch (e) {
    console.log(`FAIL ${label}: baseline threw ${e.message}`);
    fail++;
    return;
  }
  if (eq(o, b)) pass++;
  else {
    fail++;
    console.log(`FAIL ${label}: outputs differ`);
    for (const k of new Set([...Object.keys(o), ...Object.keys(b)])) {
      if (o[k] !== b[k]) {
        console.log(`  path ${k}:`);
        console.log(`    ours:     ${JSON.stringify((o[k] ?? '').slice(0, 200))}`);
        console.log(`    baseline: ${JSON.stringify((b[k] ?? '').slice(0, 200))}`);
      }
    }
  }
}

const site = (over = {}) => ({ title: 'T & Co.', base_url: 'https://x.example/blog/', per_page: 2, recent: 2, ...over });
const md = (fm, body) => `---\n${fm}\n---\n${body}\n`;
const post1 = md('title: First Post\ndate: 2026-01-05\ntags: [JavaScript, Café Culture]', 'Hello *world*.\n');
const post2 = md('title: Second Post\ndate: 2026-01-04\ntags: [javascript, Rust]', 'Body two.\n');
const page1 = md('title: About Page', '# About\n\nSome *text* here.\n');

// C1: a constant partial included at depth 1 (inside a non-constant wrapper
// that reads page) — it must render fresh there, not from the pre-render.
check('constant-at-depth-1', {
  site: site(), templates: {
    post: '{% include "wrap" %}\n{{{ page.content }}}\n',
    page: '{% include "wrap" %}\n{{{ page.content }}}\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
    wrap: '<div>{{ page.title }}|{% include "badge" %}</div>\n',
    badge: '<b>{{ site.title }}</b>{% for t in tags %}[{{ t.name }}]{% endfor %}\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'about.md', content: page1 },
  ],
});

// C2: one template used as a layout AND included by another layout. As a
// layout it renders at depth 0; as an include it reads the pre-render
// (depth 1) — the two uses may legitimately differ on deep chains.
{
  const t = {
    post: '{% include "side" %}\n{{{ page.content }}}\n',
    page: '{% include "side" %}\n{{{ page.content }}}\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
    side: 'S[{{ site.title }}|{% include "s1" %}]',
  };
  for (let k = 1; k <= 10; k++) t[`s${k}`] = k === 10 ? 'END' : `L${k}[{% include "s${k + 1}" %}]`;
  check('constant-as-layout-and-include', {
    site: site(), templates: t,
    files: [
      { path: 'posts/2026/first.md', content: post1 },
      // A page whose layout IS "side": renders it fresh at depth 0.
      { path: 'other.md', content: md('title: Other\nlayout: side', 'Other body.\n') },
      { path: 'about.md', content: page1 },
    ],
  });
}

// C3: a constant partial reading the loop element `p` (bound by a for over
// `recent`) — not constant, must render fresh per iteration.
check('constant-partial-reads-loop-element', {
  site: site(), templates: {
    post: '{% for p in recent %}{% include "row" %}{% endfor %}\n{{{ page.content }}}\n',
    page: 'P\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
    row: '{{ p.title }}|{{ site.title }};\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
  ],
});

// C4: a constant template including a missing template (renders nothing) and
// reading only constant roots.
check('constant-with-missing-include', {
  site: site(), templates: {
    post: '{% include "c" %}\n{{{ page.content }}}\n',
    page: '{% include "c" %}\n{{{ page.content }}}\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
    c: 'C[{{ site.title }}|{% include "nope" %}|{% for t in tags %}{{ t.slug }};{% endfor %}]\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'about.md', content: page1 },
  ],
});

// C5: recent is empty (recent: 0): a constant partial gated on {% if recent %}
// must render the same empty result from the pre-render.
check('constant-if-recent-empty', {
  site: site({ recent: 0 }), templates: {
    post: '{% include "n" %}\n{{{ page.content }}}\n',
    page: '{% include "n" %}\n{{{ page.content }}}\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
    n: '{% if recent %}HAS{% else %}NONE{% endif %}|{{ site.title }}\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'about.md', content: page1 },
  ],
});

// C6: a constant template nested two includes deep under a non-constant
// layout, with the middle template also constant.
check('constant-two-deep', {
  site: site(), templates: {
    post: '{% include "mid" %}\n{{{ page.content }}}\n',
    page: 'P\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
    mid: 'M[{% include "deep" %}]',
    deep: 'D[{{ site.title }}|{% for t in tags %}{{ t.name }},{% endfor %}]',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
  ],
});

// C7: a `for` over `posts` (non-constant) inside a layout, including a
// constant partial — the partial reads only constant roots, so the pre-render
// still applies inside the loop.
check('constant-inside-posts-loop', {
  site: site(), templates: {
    post: 'P\n', page: 'P\n',
    index: '{% for p in posts %}{{ p.title }}:{% include "c" %};{% endfor %}\n',
    tag: 'T\n', tags: 'G\n',
    c: '<{{ site.title }}>\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
  ],
});

console.log(`\nconstancy: ${pass} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
