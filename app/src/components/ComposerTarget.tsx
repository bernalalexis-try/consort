import type { Message } from "../lib/api";
import { useRoomLinks } from "../lib/roomLinks";
import { ReplyIcon, previewOf } from "./MessageGroups";
import "./ComposerTarget.css";

/**
 * The mark on the line when the box is correcting a message.
 *
 * The same pencil the action row draws, at the size a label on the composer
 * wants rather than the size of a glyph in the hover row over a message.
 */
function EditingIcon({ className }: { className: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M17 3a2.8 2.8 0 0 1 4 4L7.5 20.5 2 22l1.5-5.5z" />
      <path d="m15 5 4 4" />
    </svg>
  );
}

/**
 * What the box is about to do to a message, above the box that does it.
 *
 * Its own component because the room and the thread panel both have a composer
 * with these two modes, and a second copy of this line is a second copy to
 * keep in step. One treatment for a reply and an edit, because the two are the
 * same statement about the same box, and they are never both on: setting
 * either clears the other.
 *
 * An edit quotes the message as it stands rather than as it will be. What the
 * box holds is the correction being written, and having both would be the same
 * sentence twice.
 */
export function ComposerTarget({
  message,
  onStop,
  ...what
}: {
  /** The message the composer is pointed at. */
  message: Message;
  /** Put the composer back to an ordinary message. */
  onStop: () => void;
} & (
  | {
      doing: "reply";
      /** What to call whoever wrote it, which the answer will name. */
      who: string;
    }
  | { doing: "edit" }
)) {
  const { nameOf } = useRoomLinks();
  const replying = what.doing === "reply";

  return (
    <div className="composer-target">
      {replying ? (
        <ReplyIcon className="composer-target__glyph" />
      ) : (
        <EditingIcon className="composer-target__glyph" />
      )}
      <span className="composer-target__who">
        {/*
          Whose message it is, for a reply, because the answer names them.
          "Editing" for a correction, because an edit is always the reader's
          own message and repeating their name back at them says nothing.
        */}
        {what.doing === "reply" ? what.who : "Editing"}
      </span>
      <span className="composer-target__said">
        {previewOf(message, nameOf)}
      </span>
      <button
        type="button"
        className="composer-target__stop"
        aria-label={replying ? "Stop replying" : "Stop editing"}
        onClick={onStop}
      >
        &times;
      </button>
    </div>
  );
}
