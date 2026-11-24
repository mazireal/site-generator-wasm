// Stress test: my solve.js vs the baseline makefun-site on generated sets
// (practice + extra sets drawn by the kit's generator) and on hand-crafted
// edge cases (front matter, template dialect, Markdown, slug rule, statics,
// and the cases that exercise the optimizer's constant-template caching:
// loop vars shadowing `site`/`tags`/`recent`, cyclic includes, deep include
// chains, `loop`/`page` read from partials).
//
// Usage: node run.mjs [generated-set-dir ...]
//   Each dir holds input/cases.json + reference/expected.json (as drawn by
//   program/generator/draw.ts). With no args it runs the hand-crafted cases
//   only. Exits 1 when any case differs.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const KIT = '/home/hatch/workspace/makefun/site-generator-speed-01-run-20261122-kit';
const ours = await import(join(HERE, '..', 'solve.js'));
const base = await import(join(KIT, 'baselines', 'makefun-site', 'solve.js'));

const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);
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
    const ok = Object.keys(o), bk = Object.keys(b);
    for (const k of new Set([...ok, ...bk])) {
      if (o[k] !== b[k]) {
        console.log(`  path ${k}:`);
        console.log(`    ours:     ${JSON.stringify((o[k] ?? '').slice(0, 200))}`);
        console.log(`    baseline: ${JSON.stringify((b[k] ?? '').slice(0, 200))}`);
      }
    }
  }
}

// ---------------------------------------------------------------- generated sets
for (const dir of process.argv.slice(2)) {
  const cases = JSON.parse(readFileSync(join(dir, 'input', 'cases.json'), 'utf8'));
  const expected = JSON.parse(readFileSync(join(dir, 'reference', 'expected.json'), 'utf8')).cases;
  for (let i = 0; i < cases.length; i++) {
    const out = ours.run(structuredClone(cases[i].input));
    if (eq(out, expected[i].output)) pass++;
    else {
      fail++;
      console.log(`FAIL generated ${dir} ${cases[i].id}`);
    }
  }
  console.log(`generated set ${dir}: ${cases.length} cases checked`);
}

// ---------------------------------------------------------------- hand-crafted cases
const site = (over = {}) => ({ title: 'T & Co.', base_url: 'https://x.example/blog/', per_page: 2, recent: 2, ...over });
const stdTemplates = (over = {}) => ({
  post: '{% include "head" %}\n<h1>{{ page.title }}</h1>\n{{{ page.content }}}\n{% include "foot" %}\n',
  page: '{% include "head" %}\n<h1>{{ page.title }}</h1>\n{{{ page.content }}}\n{% include "foot" %}\n',
  index: '{% for p in posts %}{% include "card" %}{% endfor %}{% if not posts %}<p>Nothing here yet.</p>{% endif %}\n{% include "pager" %}\n',
  tag: '<h1>{{ tag.name }} {{ tag.count }}</h1>\n{% for p in tag.posts %}{% include "card" %}{% endfor %}\n',
  tags: '<ul>{% for t in tags %}<li>{{ t.name }} {{ t.count }}</li>{% endfor %}</ul>\n',
  head: '<title>{{ site.title }}</title>\n',
  foot: '<footer>{{ site.title }}</footer>\n',
  card: '<a href="{{ p.url }}">{{ p.title }}</a> {{ p.date_long }} {{ p.reading_minutes }}min\n',
  pager: 'Page {{ pager.number }} of {{ pager.total }}\n',
  ...over,
});
const md = (fm, body) => `---\n${fm}\n---\n${body}\n`;
const post1 = md('title: First Post\ndate: 2026-01-05\ntags: [JavaScript, Café Culture]', 'Hello *world* and **bold**.\n\nSecond para with [a link](other.md) and [ext](https://e.example/x).\n');
const post2 = md('title: Second Post\ndate: 2026-01-04\ntags: [javascript, Rust]', 'Body two.\n\n> A quote\n> more quote\n\n- one\n- two\n  continued\n');
const page1 = md('title: About Page', '# About\n\nSome *text* here.\n');

check('basic', {
  site: site(), templates: stdTemplates(),
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
    { path: 'about.md', content: page1 },
    { path: 'assets/style.css', content: 'body{}\n' },
    { path: 'robots.txt', content: 'User-agent: *\n' },
  ],
});

// A: constancy killers — a loop var shadowing `site`, another shadowing `tags`.
check('constancy-killers', {
  site: site(), templates: stdTemplates({
    post: '{% include "head" %}\n{% include "shadower" %}\n<h1>{{ page.title }}</h1>\n{{{ page.content }}}\n{% include "foot" %}\n',
    shadower: '{% for site in tags %}[{{ site.name }}]{% endfor %}{% for tags in recent %}<{{ tags.title }}>{% endfor %}\n',
  }),
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
    { path: 'about.md', content: page1 },
  ],
});

// A2: a `for` binding `recent` as a loop var inside a partial.
check('constancy-recent-shadow', {
  site: site(), templates: stdTemplates({
    foot: '{% for recent in tags %}{{ recent.name }};{% endfor %}<footer>{{ site.title }}</footer>\n',
  }),
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'about.md', content: page1 },
  ],
});

// B: cyclic includes (a<->b, self-include).
check('cycles', {
  site: site(), templates: {
    post: '{% include "a" %}{% include "self" %}\n{{{ page.content }}}\n',
    page: '{% include "a" %}\n{{{ page.content }}}\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
    a: 'A[{% include "b" %}]',
    b: 'B[{% include "a" %}]',
    self: 'S[{% include "self" %}]',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'about.md', content: page1 },
  ],
});

// C: deep include chain (12 levels; the depth limit cuts at 8).
{
  const t = { post: '{% include "t1" %}\n{{{ page.content }}}\n', page: '{% include "t1" %}\n{{{ page.content }}}\n', index: 'I\n', tag: 'T\n', tags: 'G\n' };
  for (let k = 1; k <= 12; k++) t[`t${k}`] = k === 12 ? 'END' : `L${k}[{% include "t${k + 1}" %}]`;
  check('deep-chain', { site: site(), templates: t, files: [{ path: 'posts/2026/first.md', content: post1 }] });
}

// D: a constant template reached at depth (post -> deep1 -> deep2 -> nav).
check('constant-at-depth', {
  site: site(), templates: {
    post: '{% include "deep1" %}\n{{{ page.content }}}\n',
    page: '{% include "deep1" %}\n{{{ page.content }}}\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
    deep1: 'D1[{% include "deep2" %}]',
    deep2: 'D2[{% include "nav" %}]',
    nav: '<nav>{{ site.title }}{% for p in recent %}<a href="{{ p.url }}">{{ p.title }}</a>{% endfor %}</nav>\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
  ],
});

// E: `loop` and the loop element read from a partial included inside a for.
check('loop-in-partial', {
  site: site(), templates: {
    post: 'P\n', page: 'P\n',
    index: '{% for p in posts %}{% include "row" %}{% endfor %}\n',
    tag: 'T\n', tags: 'G\n',
    row: '{{ loop.index }}/{{ loop.length }}:{{ p.title }}:{{ loop.first }}:{{ loop.last }};\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
    { path: 'posts/2026/third.md', content: md('title: Third\ndate: 2026-01-03', 'Third body.\n') },
  ],
});

// E2: a partial reading `page` (not constant) and `posts` (index-only).
check('page-in-partial', {
  site: site(), templates: stdTemplates({
    head: '<title>{{ page.title }} | {{ site.title }}</title>\n{% if page %}<meta name="x" content="{{ page.slug }}">{% endif %}\n',
    foot: '{% if posts %}has-posts{% endif %}<footer>{{ site.title }}</footer>\n',
  }),
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'about.md', content: page1 },
  ],
});

// F: degenerate inputs.
check('empty-everything', { site: site(), templates: {}, files: [] });
check('no-templates', { site: site(), templates: {}, files: [{ path: 'posts/2026/first.md', content: post1 }] });
check('empty-md', { site: site(), templates: stdTemplates(), files: [{ path: 'x.md', content: '' }] });
check('fm-only', { site: site(), templates: stdTemplates(), files: [{ path: 'x.md', content: '---\ntitle: Only FM\n---\n' }] });
check('statics-only', { site: site(), templates: stdTemplates(), files: [{ path: 'a.txt', content: 'hi\n' }, { path: 'b/c.txt', content: 'yo\n' }] });
check('no-posts', { site: site({ recent: 0 }), templates: stdTemplates(), files: [{ path: 'about.md', content: page1 }] });
check('recent-zero', { site: site({ recent: 0, per_page: 100 }), templates: stdTemplates(), files: [{ path: 'posts/2026/first.md', content: post1 }] });

// G: front matter + slugs.
check('front-matter', {
  site: site(), templates: stdTemplates(),
  files: [
    { path: 'posts/2026/uni.md', content: md('title: "Café \\"Moskva\\" — Москва 東京"\ndate: 2024-02-29\ntags: [Écriture, ecriture, Café Culture]', 'Uni body.\n') },
    { path: 'posts/2026/dup1.md', content: md('title: Same Title\ndate: 2026-01-05', 'Dup one.\n') },
    { path: 'posts/2026/dup2.md', content: md('title: SAME TITLE!!\ndate: 2026-01-05', 'Dup two.\n') },
    { path: 'posts/2026/dup3.md', content: md('title: "  Same Title."\ndate: 2026-01-05', 'Dup three.\n') },
    { path: 'posts/2026/longslug.md', content: md('title: A very long title that keeps going and going and going and going and going on\ndate: 2026-01-05', 'Long.\n') },
    { path: 'posts/2026/explicit.md', content: md('title: Whatever\ndate: 2026-01-05\nslug: my-slug', 'Explicit.\n') },
    { path: 'posts/2026/crlf.md', content: '---\r\ntitle: CRLF Post\r\ndate: 2026-01-05\r\n---\r\nCRLF body *em*.\r\n' },
    { path: 'posts/2026/bad1.md', content: md('title: Nope\ndate: 2026-02-30', 'Bad date.\n') },
    { path: 'posts/2026/bad2.md', content: md('title: Nope\ndate: 1969-12-31', 'Bad date.\n') },
    { path: 'posts/2026/bad3.md', content: md('author: someone\ntitle: Nope', 'Unknown key.\n') },
    { path: 'posts/2026/bad4.md', content: md('title: Nope\ntitle: Twice', 'Dup key.\n') },
    { path: 'posts/2026/bad5.md', content: md('title: "unclosed', 'Unclosed quote.\n') },
    { path: 'posts/2026/bad6.md', content: md('tags: [one, "two"]\ntitle: Nope', 'Bad tags.\n') },
    { path: 'posts/2026/bad7.md', content: md('draft: yes\ntitle: Nope', 'Bad draft.\n') },
    { path: 'posts/2026/bad8.md', content: md('slug: Not A Slug\ntitle: Nope', 'Bad slug.\n') },
    { path: 'posts/2026/bad9.md', content: md('summary:\ntitle: Nope', 'No summary value.\n') },
    { path: 'posts/2026/bad10.md', content: md('Title: Capitalised\ntitle: Nope', 'Capital key.\n') },
    { path: 'posts/2026/nodate.md', content: md('title: No Date Post', 'A post with no date.\n') },
    { path: 'posts/2026/nolayout.md', content: md('title: No Layout\ndate: 2026-01-05\nlayout: no-such', 'No layout.\n') },
    { path: 'posts/2026/draft.md', content: md('title: Draft Post\ndate: 2026-01-05\ndraft: true', 'Draft body.\n') },
    { path: 'posts/2026/unclosed.md', content: '---\ntitle: Never Closed\n\nBody.\n' },
    { path: 'posts/2026/nofm.md', content: 'Just text, no front matter.\n' },
    { path: 'tags/sneaky.md', content: md('title: Under tags/\ndate: 2026-01-05', 'Sneaky.\n') },
    { path: 'page/sneaky.md', content: md('title: Under page/\ndate: 2026-01-05', 'Sneaky.\n') },
    { path: 'docs/guide.md', content: md('title: Guide\ndate: 2026-01-05\ntags: []', 'Guide body.\n') },
  ],
});

// H: Markdown edges.
check('markdown-edges', {
  site: site(), templates: stdTemplates(),
  files: [{
    path: 'posts/2026/edge.md',
    content: md('title: Edge\ndate: 2026-01-05', [
      'Unclosed **bold and *em and `code here.',
      '',
      '*a **b* c** and **x *y* z** and \\*escaped\\* and \\> quoted.',
      '',
      '[x](a&b.md) [y](target with space) [z]() [w](#frag) [v](missing.md#frag) [u](./other.md)',
      '',
      '> quote line',
      '>',
      '> ',
      '> last',
      '',
      '- a',
      '* b',
      '  continued line',
      '1. one',
      '5. five',
      '10. ten',
      '',
      '```js  ',
      'const x = a < b && c > d; // "quoted" \'single\'',
      '```',
      '',
      '``` unclosed fence',
      'still code',
      '',
      'Text then setext',
      '---',
      'More text',
      '===',
      'Even more',
      '***',
      '',
      '#word not a heading',
      '####### seven hashes',
      '## Heading ## with closing',
      '    indented line',
      '\t tab indented',
      '',
      '1) not a list',
      '+ also not a list',
      '_under_ scores and __double__',
      '',
      'a < b > c & d "e" \'f\'',
      '',
      '---  ',
      '***',
    ].join('\n')),
  }, { path: 'posts/2026/other.md', content: md('title: Other\ndate: 2026-01-01', 'Other.\n') }],
});

// I: links of every kind.
check('links', {
  site: site(), templates: stdTemplates(),
  files: [
    {
      path: 'docs/a.md',
      content: md('title: Links\ndate: 2026-01-05', [
        '[up](../posts/2026/first.md) [same](first.md) [root](/posts/2026/first.md) [frag](#here)',
        '[static](../assets/pic.svg) [static-frag](../assets/pic.svg#x) [missing](../nope.md)',
        '[draft](../posts/2026/draft.md) [refused](../posts/2026/bad1.md)',
        '[mail](mailto:a@b.example) [tel](tel:+123) [query](https://q.example/a?x=1&y=2)',
        '[dot](./first.md) [dotdot](../../posts/2026/first.md) [above](../../../escape.md)',
        '[empty]() [justfrag](#) [colon](a:b) [upper](../POSTS/2026/first.md)',
      ].join('\n')),
    },
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/draft.md', content: md('title: Draft\ndate: 2026-01-05\ndraft: true', 'Draft.\n') },
    { path: 'posts/2026/bad1.md', content: md('title: Bad\ndate: 2026-02-30', 'Bad.\n') },
    { path: 'assets/pic.svg', content: '<svg/>\n' },
  ],
});

// J: shadowed static files.
check('shadowed-statics', {
  site: site(), templates: stdTemplates(),
  files: [
    { path: 'about.md', content: page1 },
    { path: 'about/index.html', content: 'static about\n' },
    { path: 'index.html', content: 'static index\n' },
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'tags/index.html', content: 'static tags\n' },
  ],
});

// K: template dialect edges — unclosed tags, bad paths, else/endif mismatches,
// raw vs escaped vars, arrays/numbers/booleans as values, nested fors.
check('template-dialect', {
  site: site(), templates: {
    post: '{% include "head" %}\n<h1>{{ page.title }}</h1>\n<p>{{ page.reading_minutes }} {{ page.is_post }} {{ page.missing }} {{ page.tags }}</p>\n{{{ page.excerpt }}}\n{% include "foot" %}\n',
    page: 'PAGE {{ page.title }}\n',
    index: '{% for p in posts %}{{ p.title }}|{{ loop.index }}|{% endfor %}{% if posts %}Y{% else %}N{% endif %}{% if not posts %}N2{% endif %}\n',
    tag: '{% if tag.posts %}has{% endif %}{{ tag.count }}\n',
    tags: '{% for t in tags %}{{ t.name }}={{ t.count }};{% endfor %}\n',
    head: '<title>{{ site.title }}</title>\n',
    foot: '<footer>{{ site.title }}</footer>\n',
    broken1: 'x {{ }} y',
    broken2: 'x {{ 1abc }} y',
    broken3: 'x {% for p in %} y {% endfor %}',
    broken4: 'x {% if %} y {% endif %}',
    broken5: 'x {% unknown %} y',
    broken6: 'x {% for p in posts %} y',
    broken7: 'x {% endfor %} y',
    broken8: 'x {% if page %} y {% else %} z {% else %} w {% endif %}',
    broken9: 'x {{ page.title } y',
    broken10: 'x {{{ page.title }} y',
    broken11: 'x {% include "missing" %} y',
    broken12: 'x {% include "broken1" %} y',
    unclosed: 'x {% if page.title %\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
    { path: 'about.md', content: page1 },
  ],
});

// K2: broken templates as the LAYOUT itself (layout "is no template that parses").
check('broken-layout', {
  site: site(), templates: {
    post: 'x {{ }} y',
    page: 'ok {{ page.title }}\n',
    index: 'I\n', tag: 'T\n', tags: 'G\n',
  },
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'about.md', content: page1 },
  ],
});

// K3: a constant-looking template including a partial that reads `page`
// (transitively not constant) — must render fresh per page.
check('transitive-not-constant', {
  site: site(), templates: stdTemplates({
    head: '<title>{{ site.title }}</title>\n{% include "meta" %}\n',
    meta: '{% if page %}<meta name="og" content="{{ page.title }}">{% endif %}\n',
  }),
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'posts/2026/second.md', content: post2 },
    { path: 'about.md', content: page1 },
  ],
});

// K4: the layout itself binds `site` in a for around a constant include —
// the global roots check must disable all caching.
check('layout-shadows-site', {
  site: site(), templates: stdTemplates({
    post: '{% for site in tags %}{% include "head" %}[{{ site.name }}]{% endfor %}\n<h1>{{ page.title }}</h1>\n{{{ page.content }}}\n{% include "foot" %}\n',
  }),
  files: [
    { path: 'posts/2026/first.md', content: post1 },
    { path: 'about.md', content: page1 },
  ],
});

console.log(`\nstress: ${pass} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
