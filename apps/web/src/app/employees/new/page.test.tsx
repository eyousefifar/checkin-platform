import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const apiMock = vi.hoisted(() => vi.fn());
const routerPush = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  api: (...args: unknown[]) => apiMock(...args),
  API_URL: "http://localhost:8000",
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: routerPush }),
}));

vi.mock("next/link", () => ({
  default: ({
    href,
    children,
    ...rest
  }: {
    href: string;
    children: React.ReactNode;
    className?: string;
  }) => (
    <a href={href} {...rest}>
      {children}
    </a>
  ),
}));

import NewEmployeePage from "./page";

function fileOf(name: string) {
  return new File(["face"], name, { type: "image/jpeg" });
}

function isDisabled(el: HTMLElement) {
  return (el as HTMLButtonElement | HTMLInputElement).disabled === true;
}

describe("NewEmployeePage enrollment recovery", () => {
  const createObjectURL = vi.fn((f: Blob) => `blob:${(f as File).name}`);
  const revokeObjectURL = vi.fn();

  beforeEach(() => {
    apiMock.mockReset();
    routerPush.mockReset();
    createObjectURL.mockClear();
    revokeObjectURL.mockClear();
    // jsdom may omit these; install stubs for preview lifecycle tests.
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      writable: true,
      value: createObjectURL,
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      writable: true,
      value: revokeObjectURL,
    });
  });

  afterEach(() => {
    // leave stubs in place; next beforeEach rebinds
  });

  it("shows create failure and keeps the form active", async () => {
    apiMock.mockRejectedValueOnce(new Error("code taken"));
    render(<NewEmployeePage />);

    fireEvent.change(screen.getByLabelText(/Employee code/i), {
      target: { value: "E1" },
    });
    fireEvent.change(screen.getByLabelText(/Full name/i), {
      target: { value: "Ada" },
    });
    fireEvent.click(screen.getByTestId("save-employee"));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/code taken/);
    });
    expect(apiMock).toHaveBeenCalledTimes(1);
    expect(isDisabled(screen.getByTestId("save-employee"))).toBe(false);
    expect(screen.queryByTestId("detail-link")).toBeNull();
  });

  it("create-only success disables metadata form and links to detail", async () => {
    apiMock.mockResolvedValueOnce({ id: 42 });
    render(<NewEmployeePage />);

    fireEvent.change(screen.getByLabelText(/Employee code/i), {
      target: { value: "E42" },
    });
    fireEvent.change(screen.getByLabelText(/Full name/i), {
      target: { value: "Grace" },
    });
    fireEvent.click(screen.getByTestId("save-employee"));

    await waitFor(() => {
      expect(screen.getByTestId("detail-link").getAttribute("href")).toBe(
        "/employees/42",
      );
    });
    expect(apiMock).toHaveBeenCalledTimes(1);
    expect(apiMock.mock.calls[0][0]).toBe("/api/employees");
    expect(isDisabled(screen.getByTestId("save-employee"))).toBe(true);
    expect(isDisabled(screen.getByLabelText(/Employee code/i))).toBe(true);
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("create + upload success shows per-file usable/rejected results", async () => {
    apiMock
      .mockResolvedValueOnce({ id: 7 })
      .mockResolvedValueOnce({
        received: 2,
        usable: 1,
        rejected: [{ filename: "bad.jpg", reason: "no_face" }],
        embedding_ready: false,
        num_images_used: 1,
        results: [
          { filename: "good.jpg", usable: true, reason: null },
          { filename: "bad.jpg", usable: false, reason: "no_face" },
        ],
        gallery_reload_pending: false,
      });

    render(<NewEmployeePage />);
    fireEvent.change(screen.getByLabelText(/Employee code/i), {
      target: { value: "E7" },
    });
    fireEvent.change(screen.getByLabelText(/Full name/i), {
      target: { value: "Lin" },
    });

    const input = screen.getByTestId("face-files") as HTMLInputElement;
    fireEvent.change(input, {
      target: { files: [fileOf("good.jpg"), fileOf("bad.jpg")] },
    });
    expect(createObjectURL).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId("file-previews").textContent).toMatch(/good\.jpg/);

    fireEvent.click(screen.getByTestId("save-employee"));

    await waitFor(() => {
      expect(screen.getByTestId("upload-results").textContent).toMatch(/good\.jpg/);
    });
    const results = screen.getByTestId("upload-results").textContent || "";
    expect(results).toMatch(/usable/);
    expect(results).toMatch(/bad\.jpg/);
    expect(results).toMatch(/no_face/);
    expect(apiMock).toHaveBeenCalledTimes(2);
    expect(apiMock.mock.calls[1][0]).toBe("/api/employees/7/images");
  });

  it("upload failure after create performs exactly one metadata POST and preserves link", async () => {
    apiMock
      .mockResolvedValueOnce({ id: 9 })
      .mockRejectedValueOnce(new Error("payload too large"));

    render(<NewEmployeePage />);
    fireEvent.change(screen.getByLabelText(/Employee code/i), {
      target: { value: "E9" },
    });
    fireEvent.change(screen.getByLabelText(/Full name/i), {
      target: { value: "Pat" },
    });
    fireEvent.change(screen.getByTestId("face-files"), {
      target: { files: [fileOf("a.jpg")] },
    });
    fireEvent.click(screen.getByTestId("save-employee"));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/payload too large/);
    });
    expect(screen.getByRole("alert").textContent).toMatch(/Employee exists/);
    expect(screen.getByTestId("detail-link").getAttribute("href")).toBe(
      "/employees/9",
    );
    expect(isDisabled(screen.getByTestId("save-employee"))).toBe(true);

    // Attempting another save must not POST metadata again.
    fireEvent.click(screen.getByTestId("save-employee"));
    expect(apiMock.mock.calls.filter((c) => c[0] === "/api/employees")).toHaveLength(
      1,
    );
  });

  it("gallery_reload_pending is success/converging, not an error", async () => {
    apiMock.mockResolvedValueOnce({ id: 3 }).mockResolvedValueOnce({
      received: 1,
      usable: 1,
      rejected: [],
      embedding_ready: true,
      num_images_used: 1,
      results: [{ filename: "ok.jpg", usable: true, reason: null }],
      gallery_reload_pending: true,
    });

    render(<NewEmployeePage />);
    fireEvent.change(screen.getByLabelText(/Employee code/i), {
      target: { value: "E3" },
    });
    fireEvent.change(screen.getByLabelText(/Full name/i), {
      target: { value: "Sam" },
    });
    fireEvent.change(screen.getByTestId("face-files"), {
      target: { files: [fileOf("ok.jpg")] },
    });
    fireEvent.click(screen.getByTestId("save-employee"));

    await waitFor(() => {
      expect(screen.getByTestId("gallery-pending")).toBeTruthy();
    });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("revokes object URLs when selection changes and on unmount", () => {
    const { unmount } = render(<NewEmployeePage />);
    const input = screen.getByTestId("face-files");
    fireEvent.change(input, { target: { files: [fileOf("one.jpg")] } });
    expect(createObjectURL).toHaveBeenCalledTimes(1);

    fireEvent.change(input, { target: { files: [fileOf("two.jpg")] } });
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:one.jpg");
    expect(createObjectURL).toHaveBeenCalledTimes(2);

    unmount();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:two.jpg");
  });
});
