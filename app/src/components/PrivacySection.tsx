import { useEffect, useState } from "react";

import {
  asCommandError,
  privacySettings,
  setPrivacySettings,
  type PrivacySettings,
} from "../lib/api";
import "./PrivacySection.css";

/**
 * What this account tells other people about itself.
 *
 * One control so far, and it is the one worth having a screen for: a read
 * receipt is the only thing Consort publishes about somebody that they did not
 * type, and it cannot be taken back once it has gone out.
 *
 * Mounted only while it is showing, like the voice section beside it, though
 * for a smaller reason: it reads the settings file on mount, and a screen
 * nobody is looking at should not be doing that.
 */
export function PrivacySection() {
  const [settings, setSettings] = useState<PrivacySettings | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void privacySettings()
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
   * Save, and draw the new answer only once it is saved.
   *
   * The other way round is worse than it looks here: a switch that moved and
   * then moved back would leave somebody unsure which of the two answers is
   * the one their account is actually using, which is the one thing a setting
   * about being watched must never be.
   */
  async function choose(publicReadReceipts: boolean) {
    setProblem(null);
    try {
      await setPrivacySettings({ publicReadReceipts });
      setSettings({ publicReadReceipts });
    } catch (raw: unknown) {
      setProblem(asCommandError(raw).message);
    }
  }

  return (
    <div className="privacy">
      {problem !== null && (
        <p className="privacy__problem" role="alert">
          {problem}
        </p>
      )}

      {settings !== null && (
        <div className="privacy__field">
          <span className="privacy__label">Read receipts</span>
          <div className="privacy__toggle">
            <input
              id="privacy-public-receipts"
              className="privacy__switch"
              type="checkbox"
              role="switch"
              aria-describedby="privacy-public-receipts-note"
              checked={settings.publicReadReceipts}
              onChange={(event) => void choose(event.target.checked)}
            />
            <label
              className="privacy__toggle-label"
              htmlFor="privacy-public-receipts"
            >
              Let people see when you have read their messages
            </label>
          </div>
          <p className="privacy__note" id="privacy-public-receipts-note">
            On, which is what every other Matrix client does: everybody in a
            room can see how far you have read in it, and there is no way to
            take a receipt back once it has been sent. Turn it off and Consort
            still keeps track of what you have read, but tells nobody, so other
            people's clients will show that you have read nothing.
          </p>
          <p className="privacy__note">
            Either way, the line marking where you stopped reading is yours
            alone. Nothing about it leaves your account.
          </p>
        </div>
      )}
    </div>
  );
}
