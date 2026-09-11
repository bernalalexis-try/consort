import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const notificationSettings = vi.hoisted(() => vi.fn());
const setNotificationSettings = vi.hoisted(() => vi.fn());
vi.mock("../lib/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/api")>()),
  notificationSettings,
  setNotificationSettings,
}));

import { NotificationsSection } from "./NotificationsSection";

const ON = { enabled: true, mentionsOnly: false, sound: true };

function drawing(): HTMLInputElement {
  return screen.getByRole("switch", { name: "Tell me when something arrives" });
}

function mentionsOnly(): HTMLInputElement {
  return screen.getByRole("switch", { name: "Only when somebody says my name" });
}

function sound(): HTMLInputElement {
  return screen.getByRole("switch", { name: "Make a sound" });
}

describe("NotificationsSection", () => {
  beforeEach(() => {
    notificationSettings.mockReset().mockResolvedValue(ON);
    setNotificationSettings.mockReset().mockResolvedValue(undefined);
  });

  it("draws what is currently chosen", async () => {
    render(<NotificationsSection />);

    await waitFor(() => expect(drawing()).toBeChecked());
    expect(mentionsOnly()).not.toBeChecked();
    expect(sound()).toBeChecked();
  });

  it("turns notifications off when asked", async () => {
    render(<NotificationsSection />);
    await waitFor(() => expect(drawing()).toBeChecked());

    await userEvent.click(drawing());

    expect(setNotificationSettings).toHaveBeenCalledWith({
      ...ON,
      enabled: false,
    });
    await waitFor(() => expect(drawing()).not.toBeChecked());
  });

  it("saves the whole section rather than the one field that changed", async () => {
    // The command takes all three, and this screen holds all three. Sending
    // one would need a merge on the other side that could lose a concurrent
    // change for no benefit.
    notificationSettings.mockResolvedValue({ ...ON, sound: false });
    render(<NotificationsSection />);
    await waitFor(() => expect(sound()).not.toBeChecked());

    await userEvent.click(mentionsOnly());

    expect(setNotificationSettings).toHaveBeenCalledWith({
      enabled: true,
      mentionsOnly: true,
      sound: false,
    });
  });

  it("dims the other two when notifications are off", async () => {
    // Rather than hiding them. What they say is still worth reading, and
    // controls that disappear leave somebody wondering what they turned off.
    notificationSettings.mockResolvedValue({ ...ON, enabled: false });
    render(<NotificationsSection />);

    await waitFor(() => expect(drawing()).not.toBeChecked());
    expect(mentionsOnly()).toBeDisabled();
    expect(sound()).toBeDisabled();
  });

  it("leaves the switch where it was when the save fails", async () => {
    setNotificationSettings.mockRejectedValue({
      message: "Your settings could not be saved.",
      detail: "read-only file system",
    });
    render(<NotificationsSection />);
    await waitFor(() => expect(drawing()).toBeChecked());

    await userEvent.click(drawing());

    expect(
      await screen.findByText("Your settings could not be saved."),
    ).toBeInTheDocument();
    expect(drawing()).toBeChecked();
  });

  it("says so when the settings cannot be read at all", async () => {
    notificationSettings.mockRejectedValue({
      message: "Consort could not read your settings.",
      detail: "no such file",
    });
    render(<NotificationsSection />);

    expect(
      await screen.findByText("Consort could not read your settings."),
    ).toBeInTheDocument();
  });

  it("says the rules come from the account rather than from here", async () => {
    // The one thing somebody has to know to use this screen: there is no room
    // list on it because the rooms are answered somewhere every client shares.
    render(<NotificationsSection />);

    expect(await screen.findAllByText(/push rules/)).not.toHaveLength(0);
  });
});
