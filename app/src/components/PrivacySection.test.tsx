import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const privacySettings = vi.hoisted(() => vi.fn());
const setPrivacySettings = vi.hoisted(() => vi.fn());
vi.mock("../lib/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/api")>()),
  privacySettings,
  setPrivacySettings,
}));

import { PrivacySection } from "./PrivacySection";

/** The one control on the screen. */
function toggle(): HTMLInputElement {
  return screen.getByRole("switch", {
    name: "Let people see when you have read their messages",
  });
}

describe("PrivacySection", () => {
  beforeEach(() => {
    privacySettings.mockReset().mockResolvedValue({ publicReadReceipts: true });
    setPrivacySettings.mockReset().mockResolvedValue(undefined);
  });

  it("draws what the account is currently doing", async () => {
    render(<PrivacySection />);

    await waitFor(() => expect(toggle()).toBeChecked());
  });

  it("draws a receipt setting that was turned off as turned off", async () => {
    privacySettings.mockResolvedValue({ publicReadReceipts: false });
    render(<PrivacySection />);

    await waitFor(() => expect(toggle()).not.toBeChecked());
  });

  it("stops publishing receipts when the switch is turned off", async () => {
    render(<PrivacySection />);
    await waitFor(() => expect(toggle()).toBeChecked());

    await userEvent.click(toggle());

    expect(setPrivacySettings).toHaveBeenCalledWith({
      publicReadReceipts: false,
    });
    await waitFor(() => expect(toggle()).not.toBeChecked());
  });

  it("leaves the switch where it was when the save fails", async () => {
    // The other way round is worse than it looks: a switch that moved and then
    // moved back leaves somebody unsure which answer their account is using,
    // which is the one thing a setting about being watched must never be.
    privacySettings.mockResolvedValue({ publicReadReceipts: false });
    setPrivacySettings.mockRejectedValue({
      message: "Your settings could not be saved.",
      detail: "read-only file system",
    });
    render(<PrivacySection />);
    await waitFor(() => expect(toggle()).not.toBeChecked());

    await userEvent.click(toggle());

    expect(
      await screen.findByText("Your settings could not be saved."),
    ).toBeInTheDocument();
    expect(toggle()).not.toBeChecked();
  });

  it("says so when the settings cannot be read at all", async () => {
    privacySettings.mockRejectedValue({
      message: "Consort could not read your settings.",
      detail: "no such file",
    });
    render(<PrivacySection />);

    expect(
      await screen.findByText("Consort could not read your settings."),
    ).toBeInTheDocument();
  });

  it("says the marker behind the line is private either way", async () => {
    // The setting is about receipts and nothing else. Somebody turning it off
    // should not be left wondering whether they have also given up knowing
    // where they stopped reading.
    render(<PrivacySection />);

    expect(
      await screen.findByText(/line marking where you stopped reading is yours/),
    ).toBeInTheDocument();
  });
});
