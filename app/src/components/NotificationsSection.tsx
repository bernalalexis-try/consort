import { useEffect, useState } from "react";

import {
  asCommandError,
  notificationSettings,
  setNotificationSettings,
  type NotificationSettings,
} from "../lib/api";
import "./NotificationsSection.css";

/**
 * When Consort interrupts somebody, and how loudly.
 *
 * Three switches and no list of rooms, deliberately. What counts as worth a
 * notification is already answered per account by Matrix push rules, which
 * every other client honours: a room muted in Element is muted here, and a
 * keyword added there works here. Rebuilding that as a second set of rules
 * Consort kept for itself would give somebody two answers to one question and
 * no way to tell which was winning.
 *
 * So what is here is only what is true of this machine: whether to interrupt
 * at all, whether to hold out for a mention, and whether the desktop makes a
 * sound doing it.
 */
export function NotificationsSection() {
  const [settings, setSettings] = useState<NotificationSettings | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void notificationSettings()
      .then((loaded) => {
        if (!cancelled) setSettings(loaded);
      })
      .catch((raw: unknown) => {
        if (!cancelled) setProblem(asCommandError(raw).message);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  /**
   * Save one field, and draw the new answer only once it is saved.
   *
   * The whole section rather than the field, because that is the shape the
   * command takes and this screen holds all of it. Drawn after the save for
   * the reason the privacy screen does the same: a switch that moved and then
   * moved back leaves somebody unsure which answer is the one in force.
   */
  async function choose(change: Partial<NotificationSettings>) {
    if (settings === null) return;

    const chosen = { ...settings, ...change };
    setProblem(null);
    try {
      await setNotificationSettings(chosen);
      setSettings(chosen);
    } catch (raw: unknown) {
      setProblem(asCommandError(raw).message);
    }
  }

  return (
    <div className="notifications">
      {problem !== null && (
        <p className="notifications__problem" role="alert">
          {problem}
        </p>
      )}

      {settings !== null && (
        <>
          <div className="notifications__field">
            <span className="notifications__label">Desktop notifications</span>
            <div className="notifications__toggle">
              <input
                id="notifications-enabled"
                className="notifications__switch"
                type="checkbox"
                role="switch"
                aria-describedby="notifications-enabled-note"
                checked={settings.enabled}
                onChange={(event) =>
                  void choose({ enabled: event.target.checked })
                }
              />
              <label
                className="notifications__toggle-label"
                htmlFor="notifications-enabled"
              >
                Tell me when something arrives
              </label>
            </div>
            <p className="notifications__note" id="notifications-enabled-note">
              Only when Consort is not the window you are looking at, or when it
              is and you are reading a different channel. Nothing is drawn about
              the conversation already on screen.
            </p>
          </div>

          <div className="notifications__field">
            <span className="notifications__label">What counts</span>
            <div className="notifications__toggle">
              <input
                id="notifications-mentions-only"
                className="notifications__switch"
                type="checkbox"
                role="switch"
                aria-describedby="notifications-mentions-only-note"
                disabled={!settings.enabled}
                checked={settings.mentionsOnly}
                onChange={(event) =>
                  void choose({ mentionsOnly: event.target.checked })
                }
              />
              <label
                className="notifications__toggle-label"
                htmlFor="notifications-mentions-only"
              >
                Only when somebody says my name
              </label>
            </div>
            <p
              className="notifications__note"
              id="notifications-mentions-only-note"
            >
              Off, so what arrives is whatever your account's push rules say.
              Those are the rules every Matrix client shares, so a room you
              muted in another client is muted here too. Turn this on to hold
              out for a mention regardless of them.
            </p>
          </div>

          <div className="notifications__field">
            <span className="notifications__label">Sound</span>
            <div className="notifications__toggle">
              <input
                id="notifications-sound"
                className="notifications__switch"
                type="checkbox"
                role="switch"
                aria-describedby="notifications-sound-note"
                disabled={!settings.enabled}
                checked={settings.sound}
                onChange={(event) =>
                  void choose({ sound: event.target.checked })
                }
              />
              <label
                className="notifications__toggle-label"
                htmlFor="notifications-sound"
              >
                Make a sound
              </label>
            </div>
            <p className="notifications__note" id="notifications-sound-note">
              Your desktop's own notification sound, not one Consort ships, so
              changing it is done where you change the rest of them. Which
              messages get one is the push rules again: a mention and a direct
              message do, a busy channel does not.
            </p>
          </div>
        </>
      )}
    </div>
  );
}
