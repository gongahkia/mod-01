import { expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { nativeWindowFixture } from './native-window-fixture.mjs';

const template = readFileSync(
  new URL('../templates/blank/cart.m01w', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n');
for (const newline of ['\n', '\r\n']) {
  it(`inserts the real first-frame program in ${newline.length === 1 ? 'LF' : 'CRLF'} checkouts`, () => {
    const input = template.replace(/\n/g, newline);
    const source = nativeWindowFixture(input, 'gui-qa-line-endings');
    expect(source).toContain('id = "gui-qa-line-endings"');
    expect(source).toContain('save_set_int("gui_ready", 73)\n  save_commit()');
    expect(source).toContain('clear(200)\n  rect_fill(8, 8, 32, 32, 7)');
    expect(source).not.toContain('clear(1)');
    expect(source).toBe(nativeWindowFixture(template, 'gui-qa-line-endings'));
  });
}

it('rejects a changed template instead of silently testing an uninstrumented game', () => {
  for (const broken of [
    template.replace('blank-cart', 'different'),
    template.replace('clear(1)', 'clear(2)'),
  ])
    expect(() => nativeWindowFixture(broken, 'gui-qa-guard')).toThrow(/template/);
});
