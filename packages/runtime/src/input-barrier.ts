import { RuntimeFault } from './errors';
import { BUTTONS, isInputFrame, type ControllerState, type InputFrame } from './input';

export interface InputBarrierSnapshot {
  readonly revision: 1;
  readonly controllers: readonly [number, number, number, number];
  readonly pointer: number;
}

export function isInputBarrierSnapshot(value: unknown): value is InputBarrierSnapshot {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const object = value as Record<string, unknown>;
  return (
    Object.keys(object).length === 3 &&
    ['revision', 'controllers', 'pointer'].every((key) => Object.hasOwn(object, key)) &&
    object.revision === 1 &&
    Array.isArray(object.controllers) &&
    object.controllers.length === 4 &&
    Object.keys(object.controllers).length === 4 &&
    Array.from(object.controllers).every(
      (mask: unknown) =>
        typeof mask === 'number' &&
        Number.isInteger(mask) &&
        mask >= 0 &&
        mask < 1 << BUTTONS.length,
    ) &&
    typeof object.pointer === 'number' &&
    Number.isInteger(object.pointer) &&
    object.pointer >= 0 &&
    object.pointer <= 3
  );
}

/** Loads quarantine already-held controls until release; data reloads retain this owner unchanged. */
export class InputBarrier {
  private controllers: [number, number, number, number] = [0, 0, 0, 0];
  private pointer = 0;

  public quarantine(input: InputFrame): void {
    validate(input);
    this.controllers = input.controllers.map(buttonMask) as [number, number, number, number];
    this.pointer = Number(input.pointer.primary) | (Number(input.pointer.secondary) << 1);
  }

  public apply(input: InputFrame): InputFrame {
    validate(input);
    const controllers = input.controllers.map((controller, port) => {
      const held = buttonMask(controller);
      const blocked = (this.controllers[port] ?? 0) & held;
      this.controllers[port] = blocked;
      return {
        buttons: Object.fromEntries(
          BUTTONS.map((button, index) => [
            button,
            controller.buttons[button] && (blocked & (1 << index)) === 0,
          ]),
        ) as Record<(typeof BUTTONS)[number], boolean>,
      };
    }) as [ControllerState, ControllerState, ControllerState, ControllerState];
    this.pointer &= Number(input.pointer.primary) | (Number(input.pointer.secondary) << 1);
    return {
      controllers,
      pointer: {
        ...input.pointer,
        primary: input.pointer.primary && (this.pointer & 1) === 0,
        secondary: input.pointer.secondary && (this.pointer & 2) === 0,
      },
      ...(input.menuAction === undefined ? {} : { menuAction: input.menuAction }),
    };
  }

  public snapshot(): InputBarrierSnapshot {
    return { revision: 1, controllers: [...this.controllers], pointer: this.pointer };
  }

  public restore(value: unknown): void {
    if (!isInputBarrierSnapshot(value))
      throw new RuntimeFault('M019103', 'invalid input-owner snapshot', { start: 0, end: 0 });
    this.controllers = [...value.controllers];
    this.pointer = value.pointer;
  }
}

function buttonMask(controller: ControllerState): number {
  return BUTTONS.reduce(
    (mask, button, index) => mask | (Number(controller.buttons[button]) << index),
    0,
  );
}
function validate(input: unknown): asserts input is InputFrame {
  if (!isInputFrame(input))
    throw new RuntimeFault('M019008', 'invalid controller input frame', { start: 0, end: 0 });
}
