import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const apiMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  api: (...args: unknown[]) => apiMock(...args),
  API_URL: "http://localhost:8000",
  analyzeEnrollmentFrame: vi.fn().mockResolvedValue({
    accepted: false,
    reason: "no_face",
    bbox: null,
    yaw: null,
    face_count: 0,
  }),
}));

vi.mock("next/navigation", () => ({
  useParams: () => ({ id: "5" }),
}));

vi.mock("next/link", () => ({
  default: ({
    href,
    children,
    ...rest
  }: {
    href: string;
    children: React.ReactNode;
  }) => (
    <a href={href} {...rest}>
      {children}
    </a>
  ),
}));

import EmployeeDetailPage from "./page";

const baseEmp = {
  id: 5,
  employee_code: "E5",
  full_name: "Taylor",
  department: "Ops",
  is_active: true,
  image_count: 1,
  usable_images: 1,
  embedding_ready: true,
  num_images_used: 1,
  images: [
    { id: 11, file_path: "a.jpg", usable: true, reject_reason: null as string | null },
  ],
};

describe("EmployeeDetailPage enrollment feedback", () => {
  beforeEach(() => {
    apiMock.mockReset();
  });

  it("refreshes employee after successful upload and shows per-file results", async () => {
    apiMock
      .mockResolvedValueOnce(baseEmp)
      .mockResolvedValueOnce({
        received: 2,
        usable: 1,
        rejected: [{ filename: "dark.jpg", reason: "no_face" }],
        embedding_ready: true,
        num_images_used: 2,
        results: [
          { filename: "ok.jpg", usable: true, reason: null },
          { filename: "dark.jpg", usable: false, reason: "no_face" },
        ],
        gallery_reload_pending: false,
      })
      .mockResolvedValueOnce({
        ...baseEmp,
        image_count: 3,
        usable_images: 2,
        images: [
          ...baseEmp.images,
          { id: 12, file_path: "ok.jpg", usable: true, reject_reason: null },
          {
            id: 13,
            file_path: "dark.jpg",
            usable: false,
            reject_reason: "no_face",
          },
        ],
      });

    render(<EmployeeDetailPage />);
    await waitFor(() => {
      expect(screen.getByText("Taylor")).toBeTruthy();
    });

    fireEvent.change(screen.getByTestId("detail-files"), {
      target: {
        files: [
          new File(["a"], "ok.jpg", { type: "image/jpeg" }),
          new File(["b"], "dark.jpg", { type: "image/jpeg" }),
        ],
      },
    });
    fireEvent.click(screen.getByTestId("upload-images"));

    await waitFor(() => {
      expect(screen.getByTestId("upload-results").textContent).toMatch(/dark\.jpg/);
    });
    expect(screen.getByTestId("upload-results").textContent).toMatch(/no_face/);
    expect(screen.getByTestId("detail-status").getAttribute("role")).toBe("status");
    // Refresh GET after upload.
    expect(apiMock.mock.calls.map((c) => c[0])).toEqual([
      "/api/employees/5",
      "/api/employees/5/images",
      "/api/employees/5",
    ]);
    expect(screen.getByTestId("image-list").textContent).toMatch(/#12/);
  });

  it("upload failure preserves current employee and uses alert role", async () => {
    apiMock
      .mockResolvedValueOnce(baseEmp)
      .mockRejectedValueOnce(new Error("disk full"));

    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByText("Taylor")).toBeTruthy());

    fireEvent.change(screen.getByTestId("detail-files"), {
      target: { files: [new File(["x"], "x.jpg", { type: "image/jpeg" })] },
    });
    fireEvent.click(screen.getByTestId("upload-images"));

    await waitFor(() => {
      expect(screen.getByTestId("detail-error").textContent).toMatch(/disk full/);
    });
    expect(screen.getByTestId("detail-error").getAttribute("role")).toBe("alert");
    expect(screen.getByText("Taylor")).toBeTruthy();
    // No refresh GET after failed upload.
    expect(apiMock.mock.calls.map((c) => c[0])).toEqual([
      "/api/employees/5",
      "/api/employees/5/images",
    ]);
  });

  it("clears prior error on retry and treats gallery_reload_pending as status", async () => {
    apiMock
      .mockResolvedValueOnce(baseEmp)
      .mockRejectedValueOnce(new Error("first fail"))
      .mockResolvedValueOnce({
        received: 1,
        usable: 1,
        rejected: [],
        embedding_ready: true,
        num_images_used: 2,
        results: [{ filename: "ok.jpg", usable: true, reason: null }],
        gallery_reload_pending: true,
      })
      .mockResolvedValueOnce(baseEmp);

    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByText("Taylor")).toBeTruthy());

    fireEvent.change(screen.getByTestId("detail-files"), {
      target: { files: [new File(["x"], "ok.jpg", { type: "image/jpeg" })] },
    });
    fireEvent.click(screen.getByTestId("upload-images"));
    await waitFor(() =>
      expect(screen.getByTestId("detail-error").textContent).toMatch(/first fail/),
    );

    fireEvent.click(screen.getByTestId("upload-images"));
    await waitFor(() => {
      expect(screen.getByTestId("gallery-pending")).toBeTruthy();
    });
    expect(screen.queryByTestId("detail-error")).toBeNull();
    expect(screen.getByTestId("detail-status").getAttribute("role")).toBe("status");
  });

  it("recompute is separate and does not claim a file upload", async () => {
    apiMock
      .mockResolvedValueOnce(baseEmp)
      .mockResolvedValueOnce({
        received: 0,
        usable: 1,
        rejected: [],
        embedding_ready: true,
        num_images_used: 1,
      })
      .mockResolvedValueOnce(baseEmp);

    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByText("Taylor")).toBeTruthy());

    fireEvent.click(screen.getByTestId("recompute-embedding"));
    await waitFor(() => {
      expect(screen.getByTestId("recompute-result").textContent).toMatch(
        /usable=1/,
      );
    });
    expect(screen.getByTestId("detail-status").textContent).toMatch(
      /Recompute \(no new files\)/,
    );
    expect(apiMock.mock.calls.map((c) => c[0])).toEqual([
      "/api/employees/5",
      "/api/employees/5/recompute-embedding",
      "/api/employees/5",
    ]);
  });
});
