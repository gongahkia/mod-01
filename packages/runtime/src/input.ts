export const BUTTONS = [
  'up',
  'down',
  'left',
  'right',
  'a',
  'b',
  'x',
  'y',
  'l',
  'r',
  'start',
  'menu',
] as const;

export type Button = (typeof BUTTONS)[number];

export interface PointerState {
  readonly x: number;
  readonly y: number;
  readonly primary: boolean;
  readonly secondary: boolean;
  readonly inside: boolean;
}

export interface ControllerState {
  readonly buttons: Readonly<Record<Button, boolean>>;
}

export interface InputFrame {
  readonly controllers: readonly [
    ControllerState,
    ControllerState,
    ControllerState,
    ControllerState,
  ];
  readonly pointer: PointerState;
}

export function emptyInputFrame(): InputFrame {
  const controller = (): ControllerState => ({
    buttons: Object.fromEntries(BUTTONS.map((button) => [button, false])) as Record<
      Button,
      boolean
    >,
  });
  return {
    controllers: [controller(), controller(), controller(), controller()],
    pointer: { x: 0, y: 0, primary: false, secondary: false, inside: false },
  };
}

/** Little-endian controller/pointer MMIO over the same frames used by the high-level API. */
export function inputRegisterByte(
  current: InputFrame,
  previous: InputFrame,
  offset: number,
): number {
  if (offset < 0 || offset >= 48 || !Number.isInteger(offset)) return 0;
  if (offset < 32) {
    const port = Math.floor(offset / 8);
    const mask = (frame: InputFrame): number =>
      BUTTONS.reduce(
        (bits, button, bit) => bits | (frame.controllers[port]?.buttons[button] ? 1 << bit : 0),
        0,
      );
    const held = mask(current);
    const before = mask(previous);
    const field = offset % 8;
    const bits =
      field < 2 ? held : field < 4 ? before : field < 6 ? held & ~before : before & ~held;
    return (bits >>> ((offset % 2) * 8)) & 255;
  }
  const field = offset - 32;
  if (field < 8) {
    const pointer = field < 4 ? current.pointer : previous.pointer;
    const value = field % 4 < 2 ? pointer.x : pointer.y;
    return (value >>> ((field % 2) * 8)) & 255;
  }
  const flags = (pointer: PointerState): number =>
    Number(pointer.primary) | (Number(pointer.secondary) << 1) | (Number(pointer.inside) << 2);
  const held = flags(current.pointer);
  const before = flags(previous.pointer);
  return field === 8
    ? held
    : field === 9
      ? before
      : field === 10
        ? held & ~before
        : field === 11
          ? before & ~held
          : 0;
}

export function isButton(value: unknown): value is Button {
  return typeof value === 'string' && (BUTTONS as readonly string[]).includes(value);
}

export function isInputFrame(value: unknown): value is InputFrame {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['controllers', 'pointer']) ||
    !Array.isArray(value.controllers) ||
    value.controllers.length !== 4
  ) {
    return false;
  }
  if (
    Object.keys(value.controllers).length !== 4 ||
    !Array.from(value.controllers).every(isControllerState) ||
    !isRecord(value.pointer)
  ) {
    return false;
  }
  const pointer = value.pointer;
  return (
    hasExactKeys(pointer, ['x', 'y', 'primary', 'secondary', 'inside']) &&
    typeof pointer.x === 'number' &&
    Number.isSafeInteger(pointer.x) &&
    pointer.x >= 0 &&
    pointer.x < HARDWARE.width &&
    typeof pointer.y === 'number' &&
    Number.isSafeInteger(pointer.y) &&
    pointer.y >= 0 &&
    pointer.y < HARDWARE.height &&
    typeof pointer.primary === 'boolean' &&
    typeof pointer.secondary === 'boolean' &&
    typeof pointer.inside === 'boolean'
  );
}

export interface ControllerProfile {
  readonly revision: 1;
  readonly name: string;
  readonly keyboard: Readonly<Record<string, readonly [number, Button]>>;
  /** Physical standard-gamepad index assigned to each logical port; null disconnects that slot. */
  readonly gamepads: readonly [number | null, number | null, number | null, number | null];
}

const DEFAULT_KEY_BINDINGS: Readonly<Record<string, readonly [number, Button]>> = Object.freeze({
  ArrowUp: [0, 'up'],
  ArrowDown: [0, 'down'],
  ArrowLeft: [0, 'left'],
  ArrowRight: [0, 'right'],
  KeyZ: [0, 'a'],
  KeyX: [0, 'b'],
  KeyA: [0, 'x'],
  KeyS: [0, 'y'],
  KeyQ: [0, 'l'],
  KeyW: [0, 'r'],
  Enter: [0, 'start'],
  Escape: [0, 'menu'],
  KeyI: [1, 'up'],
  KeyK: [1, 'down'],
  KeyJ: [1, 'left'],
  KeyL: [1, 'right'],
  KeyF: [1, 'a'],
  KeyG: [1, 'b'],
  KeyR: [1, 'x'],
  KeyT: [1, 'y'],
  KeyV: [1, 'l'],
  KeyB: [1, 'r'],
  Digit1: [1, 'start'],
  Backquote: [1, 'menu'],
  Numpad8: [2, 'up'],
  Numpad5: [2, 'down'],
  Numpad4: [2, 'left'],
  Numpad6: [2, 'right'],
  Numpad1: [2, 'a'],
  Numpad2: [2, 'b'],
  Numpad7: [2, 'x'],
  Numpad9: [2, 'y'],
  NumpadAdd: [2, 'l'],
  NumpadSubtract: [2, 'r'],
  NumpadEnter: [2, 'start'],
  NumpadDecimal: [2, 'menu'],
  KeyY: [3, 'up'],
  KeyH: [3, 'down'],
  KeyU: [3, 'left'],
  KeyO: [3, 'right'],
  KeyC: [3, 'a'],
  KeyD: [3, 'b'],
  KeyE: [3, 'x'],
  KeyM: [3, 'y'],
  KeyN: [3, 'l'],
  KeyP: [3, 'r'],
  Digit2: [3, 'start'],
  Digit3: [3, 'menu'],
});

export function defaultControllerProfile(): ControllerProfile {
  return {
    revision: 1,
    name: 'DEFAULT',
    keyboard: structuredClone(DEFAULT_KEY_BINDINGS),
    gamepads: [0, 1, 2, 3],
  };
}

export function isControllerProfile(value: unknown): value is ControllerProfile {
  if (
    !isRecord(value) ||
    value.revision !== 1 ||
    typeof value.name !== 'string' ||
    value.name.length === 0 ||
    value.name.length > 24 ||
    !isRecord(value.keyboard) ||
    Object.keys(value.keyboard).length > BUTTONS.length * 4 ||
    !Array.isArray(value.gamepads) ||
    value.gamepads.length !== 4 ||
    Object.keys(value.gamepads).length !== 4
  )
    return false;
  const assignments = new Set<string>();
  for (const [code, binding] of Object.entries(value.keyboard)) {
    if (
      !/^\w{1,32}$/.test(code) ||
      !Array.isArray(binding) ||
      binding.length !== 2 ||
      !Number.isSafeInteger(binding[0]) ||
      binding[0] < 0 ||
      binding[0] > 3 ||
      !isButton(binding[1])
    )
      return false;
    const target = `${String(binding[0])}/${binding[1]}`;
    if (assignments.has(target)) return false;
    assignments.add(target);
  }
  const connected = value.gamepads.filter((index): index is number => index !== null);
  return (
    connected.every((index) => Number.isSafeInteger(index) && index >= 0 && index <= 255) &&
    new Set(connected).size === connected.length
  );
}

export function remapControllerKey(
  profile: ControllerProfile,
  code: string,
  port: number,
  button: Button,
): ControllerProfile {
  if (!isControllerProfile(profile) || !/^\w{1,32}$/.test(code) || port < 0 || port > 3)
    throw new TypeError('invalid controller remap');
  const keyboard = Object.fromEntries(
    Object.entries(profile.keyboard).filter(
      ([existingCode, binding]) =>
        existingCode !== code && !(binding[0] === port && binding[1] === button),
    ),
  );
  keyboard[code] = [port, button] as const;
  const next = { ...profile, keyboard };
  if (!isControllerProfile(next))
    throw new TypeError('controller remap conflicts with this profile');
  return next;
}

const GAMEPAD_BUTTONS: Readonly<Record<Button, number>> = Object.freeze({
  up: 12,
  down: 13,
  left: 14,
  right: 15,
  a: 0,
  b: 1,
  x: 2,
  y: 3,
  l: 4,
  r: 5,
  start: 9,
  menu: 8,
});

export interface StandardGamepad {
  readonly index: number;
  readonly buttons: readonly { readonly pressed: boolean }[];
  readonly axes: readonly number[];
}

export type GamepadProvider = () => readonly (StandardGamepad | null)[];

export function profiledControllerButtons(
  profile: ControllerProfile,
  keys: Iterable<string>,
  gamepads: readonly (StandardGamepad | null)[],
): readonly [
  Record<Button, boolean>,
  Record<Button, boolean>,
  Record<Button, boolean>,
  Record<Button, boolean>,
] {
  if (!isControllerProfile(profile)) throw new TypeError('invalid controller profile');
  const buttons = Array.from({ length: 4 }, () => emptyButtons());
  for (const code of keys) {
    const binding = profile.keyboard[code];
    if (binding === undefined) continue;
    const controller = buttons[binding[0]];
    if (controller !== undefined) controller[binding[1]] = true;
  }
  for (const gamepad of gamepads) {
    if (gamepad === null) continue;
    const port = profile.gamepads.indexOf(gamepad.index);
    const controller = port < 0 ? undefined : buttons[port];
    if (controller !== undefined) applyStandardGamepad(controller, gamepad);
  }
  return buttons as [
    Record<Button, boolean>,
    Record<Button, boolean>,
    Record<Button, boolean>,
    Record<Button, boolean>,
  ];
}

export function standardGamepadButtons(
  gamepad: StandardGamepad,
): Readonly<Record<Button, boolean>> {
  const result = emptyButtons();
  applyStandardGamepad(result, gamepad);
  return result;
}

function emptyButtons(): Record<Button, boolean> {
  return Object.fromEntries(BUTTONS.map((button) => [button, false])) as Record<Button, boolean>;
}

function applyStandardGamepad(buttons: Record<Button, boolean>, gamepad: StandardGamepad): void {
  for (const button of BUTTONS) {
    buttons[button] ||= gamepad.buttons[GAMEPAD_BUTTONS[button]]?.pressed ?? false;
  }
  const horizontal = gamepad.axes[0] ?? 0;
  const vertical = gamepad.axes[1] ?? 0;
  buttons.left ||= horizontal < -0.5;
  buttons.right ||= horizontal > 0.5;
  buttons.up ||= vertical < -0.5;
  buttons.down ||= vertical > 0.5;
}

function isControllerState(value: unknown): value is ControllerState {
  if (!isRecord(value) || !hasExactKeys(value, ['buttons']) || !isRecord(value.buttons)) {
    return false;
  }
  const buttons = value.buttons;
  return (
    hasExactKeys(buttons, BUTTONS) &&
    BUTTONS.every((button) => typeof buttons[button] === 'boolean')
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => keys.includes(key));
}
import { HARDWARE } from './hardware';
