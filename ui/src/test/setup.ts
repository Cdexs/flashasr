/// <reference types="vitest/globals" />
import "@testing-library/jest-dom";

// 单测里没有 Tauri 后端：invoke 一律返回 null（各测试按需覆盖）。
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => null),
}));
