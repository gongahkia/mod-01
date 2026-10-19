import { RuntimeFault } from './errors';
import type { SourceSpan } from './protocol';

export interface MenuItem {
  readonly slot: number;
  readonly label: string;
}

export function isMenuAction(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 1 && value <= 5;
}

export function isMenuLabel(value: unknown): value is string {
  return typeof value === 'string' && value.length <= 24 && !/[^\x20-\x7e]/.test(value);
}

export function isMenuSnapshot(value: unknown): value is readonly (string | null)[] {
  return (
    Array.isArray(value) &&
    value.length === 5 &&
    Object.keys(value).length === 5 &&
    Array.from(value).every((label) => label === null || (isMenuLabel(label) && label.length > 0))
  );
}

/** Cartridge-owned, bounded registrations. The host menu never executes a cartridge callback. */
export class CartridgeMenu {
  private labels: Array<string | null> = [null, null, null, null, null];

  public set(
    slot: unknown,
    label: unknown,
    span: SourceSpan,
    charge: (units: number, span: SourceSpan) => void,
  ): void {
    if (!isMenuAction(slot) || !isMenuLabel(label))
      throw new RuntimeFault(
        'M019016',
        'menu_item needs slot 1–5 and 0–24 printable ASCII characters',
        span,
      );
    charge(1 + label.length, span);
    this.labels[slot - 1] = label.length === 0 ? null : label;
  }

  public requireAction(slot: number): void {
    if (!isMenuAction(slot) || this.labels[slot - 1] === null)
      throw new RuntimeFault('M019016', 'selected menu action is not registered', {
        start: 0,
        end: 0,
      });
  }

  public items(): readonly MenuItem[] {
    return this.labels.flatMap((label, index) =>
      label === null ? [] : [{ slot: index + 1, label }],
    );
  }

  public snapshot(): readonly (string | null)[] {
    return [...this.labels];
  }

  public restore(labels: readonly (string | null)[]): void {
    this.labels = [...labels];
  }
}
