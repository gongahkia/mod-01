import { strict as assert } from 'node:assert';

export function nativeWindowFixture(template, id) {
  const normalized = template.replace(/\r\n/g, '\n');
  assert.ok(
    normalized.includes('id = "blank-cart"') && normalized.includes('on draw:\n  clear(1)'),
    'GUI QA template changed; update the initializer instead of testing an uninstrumented game',
  );
  return normalized
    .replace('id = "blank-cart"', `id = ${JSON.stringify(id)}`)
    .replace(
      'on draw:\n  clear(1)',
      'on start:\n  save_set_int("gui_ready", 73)\n  save_commit()\non draw:\n  clear(200)\n  rect_fill(8, 8, 32, 32, 7)',
    );
}
