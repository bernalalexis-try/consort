/**
 * Going back and forward the way the buttons on a mouse do.
 *
 * `popstate` arrives on a later task, in jsdom as in a browser, so a `back()`
 * has not reached anything by the time the call returns. Waiting on turns of
 * the task queue rather than on a delay, which would be a number tuned on one
 * machine and flaky on another.
 */
import { act } from "@testing-library/react";

async function traversed(): Promise<void> {
  await act(async () => {
    for (let turn = 0; turn < 5; turn += 1) {
      await new Promise((settle) => setTimeout(settle, 0));
    }
  });
}

export async function goBack(): Promise<void> {
  act(() => window.history.back());
  await traversed();
}

export async function goForward(): Promise<void> {
  act(() => window.history.forward());
  await traversed();
}

/**
 * The extra buttons on a mouse, as the DOM numbers them.
 *
 * 3 and 4 are what the UI Events specification calls the fourth and fifth
 * buttons, which every mouse that has them ships as Back and Forward.
 */
const BACK = 3;
const FORWARD = 4;

async function press(button: number): Promise<void> {
  act(() => {
    window.dispatchEvent(
      new MouseEvent("mousedown", { button, bubbles: true, cancelable: true }),
    );
  });
  await traversed();
}

export async function pressBack(): Promise<void> {
  await press(BACK);
}

export async function pressForward(): Promise<void> {
  await press(FORWARD);
}

/** An ordinary left click, which must not be mistaken for either of those. */
export async function pressPrimary(): Promise<void> {
  await press(0);
}
