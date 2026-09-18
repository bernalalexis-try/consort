import { useCallback, useEffect, useState } from "react";

import { LoginScreen } from "./components/LoginScreen";
import { SignedIn } from "./components/SignedIn";
import { Splash } from "./components/Splash";
import { asCommandError, quit, sessionStatus, type Profile } from "./lib/api";

type View =
  | { name: "checking" }
  | { name: "signedOut" }
  | { name: "signedIn"; profile: Profile };

export function App() {
  const [view, setView] = useState<View>({ name: "checking" });

  useEffect(() => {
    let cancelled = false;

    sessionStatus()
      .then((status) => {
        if (cancelled) return;
        setView(
          status.status === "signedIn"
            ? { name: "signedIn", profile: status.profile }
            : { name: "signedOut" },
        );
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        // The Rust side already treats an unrestorable session as signed out,
        // so reaching here means the command itself failed. There is nothing
        // useful to show but the login form.
        console.error("session_status failed", asCommandError(error));
        setView({ name: "signedOut" });
      });

    return () => {
      cancelled = true;
    };
  }, []);

  /*
    Ctrl+Q closes Consort, as it does in every other application on this
    desktop.

    Here rather than in the shell because all three views below are somewhere
    somebody can be stuck: a login that will not go through and a splash
    waiting on a homeserver are exactly when a way out is wanted, and a quit
    key that only worked once you were signed in would be missing then.

    Deliberately not filtered by what has focus, which is the one thing the
    Ctrl+V handler in `RoomTimeline` does do. A paste aimed at a settings field
    is not the room's; a quit is nobody's in particular, and a quit key that
    silently did nothing depending on where the caret sat would be worse than
    no quit key at all.

    `preventDefault` because WebKitGTK may have its own opinion about this
    combination, and the answer is ours.
  */
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (!event.ctrlKey || event.key !== "q") return;
      event.preventDefault();
      void quit();
    }

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const handleSignedIn = useCallback((profile: Profile) => {
    setView({ name: "signedIn", profile });
  }, []);

  const handleSignedOut = useCallback(() => {
    setView({ name: "signedOut" });
  }, []);

  switch (view.name) {
    case "checking":
      return <Splash />;
    case "signedOut":
      return <LoginScreen onSignedIn={handleSignedIn} />;
    case "signedIn":
      return <SignedIn profile={view.profile} onSignedOut={handleSignedOut} />;
  }
}
