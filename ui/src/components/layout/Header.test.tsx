// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import Header from "./Header";

vi.mock("@/lib/auth", () => ({
  useAuth: () => ({
    user: { username: "root", role: "system" },
    logout: vi.fn(),
  }),
}));

vi.mock("./BackendStatusIndicator", () => ({
  default: () => <div data-testid="backend-status-indicator">status</div>,
}));

vi.mock("./NotificationsDropdown", () => ({
  NotificationsDropdown: () => <div data-testid="notifications-dropdown">notifications</div>,
}));

vi.mock("./UserMenu", () => ({
  UserMenu: () => <div data-testid="user-menu">user</div>,
}));

describe("Header", () => {
  it("renders the backend status indicator before notifications", () => {
    render(<Header />);

    const indicator = screen.getByTestId("backend-status-indicator");
    const notifications = screen.getByTestId("notifications-dropdown");

    expect(indicator.compareDocumentPosition(notifications) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});