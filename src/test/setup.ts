import "@testing-library/jest-dom";

// Mock Tauri APIs not available in jsdom
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue("/mocked/path"),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
