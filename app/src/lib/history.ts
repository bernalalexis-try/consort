/**
 * Where the shell is, and how it gets back to where it was.
 *
 * The window's own history rather than a stack of our own. Those entries are
 * what the webview traverses whenever anything else asks it to, so keeping the
 * places in them leaves one history rather than two that can disagree about
 * where Back goes.
 *
 * Each entry carries the place it is for, so coming back is reading the answer
 * off the entry rather than counting steps. Nothing is written into the URL:
 * the page is served from a custom protocol whose address is the application
 * itself, and a room ID in it would be a second place to keep the selection.
 */
import { useCallback, useEffect, useRef, useState } from "react";

/** Somewhere the shell can be: a rail entry, and a room in it or none. */
export interface Where {
  spaceId: string;
  channelId: string | null;
}

/**
 * The key a place is filed under inside a history entry.
 *
 * Named rather than stored bare, because `history.state` is one object and an
 * entry that predates the shell being mounted has to be recognisable as not
 * ours. There is nowhere to go back to on one of those, and moving the screen
 * anyway would be inventing a destination.
 */
const WHERE = "consortWhere";

/**
 * The extra buttons on a mouse, as the DOM numbers them.
 *
 * Acted on here rather than left to the webview, because whether an engine
 * treats these as a navigation of its own or simply hands the page an event
 * and waits is the engine's business, and not the same everywhere. Cancelling
 * the default and traversing ourselves lands one entry back under either,
 * which is what somebody pressing the button meant by it.
 */
const BACK = 3;
const FORWARD = 4;

function samePlace(one: Where, other: Where): boolean {
  return one.spaceId === other.spaceId && one.channelId === other.channelId;
}

/** The place an entry carries, or null when the entry is not one of ours. */
function whereIn(state: unknown): Where | null {
  if (state === null || typeof state !== "object") return null;
  return (state as { [WHERE]?: Where })[WHERE] ?? null;
}

/**
 * The place the shell is at, and the way to send it somewhere else.
 *
 * Shaped like `useState` because that is what it replaces, and because every
 * caller either reads the current place or moves to a new one. The difference
 * is that moving is recorded: what came before stays reachable.
 */
export function useHistory(initial: Where): [Where, (next: Where) => void] {
  const [where, setWhere] = useState(initial);
  /*
    Where we are, for the sake of the next move. A ref as well as state because
    pushing an entry is a side effect, and an updater runs twice under
    StrictMode: deciding inside one would file the same room under two entries.
  */
  const at = useRef(where);

  /*
    The entry the window loaded with carries nothing, so a Back arriving at it
    would be a traversal with no place to restore. Marking it on mount is what
    makes wherever the shell opened somewhere it can return to.
  */
  useEffect(() => {
    window.history.replaceState({ [WHERE]: at.current }, "");
  }, []);

  useEffect(() => {
    function press(event: MouseEvent) {
      if (event.button !== BACK && event.button !== FORWARD) return;
      event.preventDefault();
      if (event.button === BACK) window.history.back();
      else window.history.forward();
    }

    window.addEventListener("mousedown", press);
    return () => window.removeEventListener("mousedown", press);
  }, []);

  useEffect(() => {
    function arrive(event: PopStateEvent) {
      const there = whereIn(event.state);
      if (there === null) return;
      at.current = there;
      setWhere(there);
    }

    window.addEventListener("popstate", arrive);
    return () => window.removeEventListener("popstate", arrive);
  }, []);

  const goTo = useCallback((next: Where) => {
    // Clicking the room you are reading is not somewhere new. An entry for it
    // would make the first press of Back a button that appears to do nothing.
    if (samePlace(at.current, next)) return;
    at.current = next;
    window.history.pushState({ [WHERE]: next }, "");
    setWhere(next);
  }, []);

  return [where, goTo];
}
