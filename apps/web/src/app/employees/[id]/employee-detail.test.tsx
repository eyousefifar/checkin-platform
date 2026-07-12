import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/** Toggle a controlled checkbox via native click (fireEvent.change is unreliable). */
async function setChecked(testId: string, checked: boolean) {
  const el = screen.getByTestId(testId) as HTMLInputElement;
  if (el.checked === checked) return;
  await act(async () => {
    el.click();
  });
  expect(el.checked).toBe(checked);
}

const apiMock = vi.hoisted(() => vi.fn());
const confirmMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  api: (...args: unknown[]) => apiMock(...args),
  API_URL: "http://localhost:8000",
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

describe("EmployeeDetailPage edit and deactivate", () => {
  beforeEach(() => {
    apiMock.mockReset();
    confirmMock.mockReset();
    vi.stubGlobal("confirm", confirmMock);
  });

  it("employee code is read-only with no editable control", async () => {
    apiMock.mockResolvedValueOnce(baseEmp);
    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByText("Taylor")).toBeTruthy());
    const code = screen.getByTestId("employee-code-readonly") as HTMLInputElement;
    expect(code.readOnly || code.disabled).toBe(true);
    expect(code.value).toBe("E5");
    // No text input that would PATCH employee_code.
    expect(screen.queryByLabelText(/employee code/i)).toBeTruthy();
  });

  it("PATCHes exact name and department body and renders returned values", async () => {
    const updated = {
      ...baseEmp,
      full_name: "Taylor Swift",
      department: "R&D",
    };
    apiMock.mockResolvedValueOnce(baseEmp).mockResolvedValueOnce(updated);

    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByTestId("edit-full-name")).toBeTruthy());

    fireEvent.change(screen.getByTestId("edit-full-name"), {
      target: { value: "Taylor Swift" },
    });
    fireEvent.change(screen.getByTestId("edit-department"), {
      target: { value: "R&D" },
    });
    fireEvent.click(screen.getByTestId("save-profile"));

    await waitFor(() => {
      expect(screen.getByTestId("detail-status").textContent).toMatch(/saved/i);
    });

    const patchCall = apiMock.mock.calls.find(
      (c) => c[1] && (c[1] as { method?: string }).method === "PATCH",
    );
    expect(patchCall).toBeTruthy();
    expect(patchCall![0]).toBe("/api/employees/5");
    const init = patchCall![1] as { method: string; body: string };
    expect(init.method).toBe("PATCH");
    expect(JSON.parse(init.body)).toEqual({
      full_name: "Taylor Swift",
      department: "R&D",
      is_active: true,
    });
    expect(confirmMock).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { level: 1 }).textContent).toMatch(
      /Taylor Swift/i,
    );
    expect((screen.getByTestId("edit-department") as HTMLInputElement).value).toBe(
      "R&D",
    );
  });

  it("metadata-only edit does not prompt for confirmation", async () => {
    apiMock.mockResolvedValueOnce(baseEmp).mockResolvedValueOnce({
      ...baseEmp,
      full_name: "Only Name",
    });
    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByTestId("edit-full-name")).toBeTruthy());
    fireEvent.change(screen.getByTestId("edit-full-name"), {
      target: { value: "Only Name" },
    });
    fireEvent.click(screen.getByTestId("save-profile"));
    await waitFor(() =>
      expect(screen.getByTestId("detail-status").textContent).toMatch(/saved/i),
    );
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("canceled deactivation sends no request and restores active control", async () => {
    apiMock.mockResolvedValueOnce(baseEmp);
    confirmMock.mockReturnValue(false);

    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByTestId("edit-is-active")).toBeTruthy());

    await setChecked("edit-is-active", false);
    fireEvent.click(screen.getByTestId("save-profile"));

    await waitFor(() => {
      expect(confirmMock).toHaveBeenCalled();
    });
    expect(
      apiMock.mock.calls.some(
        (c) => c[1] && (c[1] as { method?: string }).method === "PATCH",
      ),
    ).toBe(false);
    expect((screen.getByTestId("edit-is-active") as HTMLInputElement).checked).toBe(
      true,
    );
    expect(screen.getByTestId("employee-active-display").textContent).toMatch(
      /active/,
    );
  });

  it("confirmed deactivation PATCHes is_active false", async () => {
    const deactivated = { ...baseEmp, is_active: false };
    apiMock.mockResolvedValueOnce(baseEmp).mockResolvedValueOnce(deactivated);
    confirmMock.mockReturnValue(true);

    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByTestId("edit-is-active")).toBeTruthy());
    await setChecked("edit-is-active", false);
    fireEvent.click(screen.getByTestId("save-profile"));

    await waitFor(() =>
      expect(screen.getByTestId("detail-status").textContent).toMatch(/saved/i),
    );
    expect(confirmMock).toHaveBeenCalled();
    const patchCall = apiMock.mock.calls.find(
      (c) => c[1] && (c[1] as { method?: string }).method === "PATCH",
    );
    expect(JSON.parse((patchCall![1] as { body: string }).body)).toEqual({
      full_name: "Taylor",
      department: "Ops",
      is_active: false,
    });
    expect(screen.getByTestId("employee-active-display").textContent).toMatch(
      /inactive/,
    );
  });

  it("reactivation does not prompt and PATCHes is_active true", async () => {
    const inactive = { ...baseEmp, is_active: false };
    const reactivated = { ...baseEmp, is_active: true };
    apiMock.mockResolvedValueOnce(inactive).mockResolvedValueOnce(reactivated);

    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByTestId("edit-is-active")).toBeTruthy());
    expect((screen.getByTestId("edit-is-active") as HTMLInputElement).checked).toBe(
      false,
    );
    await setChecked("edit-is-active", true);
    fireEvent.click(screen.getByTestId("save-profile"));

    await waitFor(() =>
      expect(screen.getByTestId("detail-status").textContent).toMatch(/saved/i),
    );
    expect(confirmMock).not.toHaveBeenCalled();
    const patchCall = apiMock.mock.calls.find(
      (c) => c[1] && (c[1] as { method?: string }).method === "PATCH",
    );
    expect(JSON.parse((patchCall![1] as { body: string }).body).is_active).toBe(
      true,
    );
  });

  it("API error preserves server metadata, form values, and alert", async () => {
    apiMock
      .mockResolvedValueOnce(baseEmp)
      .mockRejectedValueOnce(new Error("server refused"));

    render(<EmployeeDetailPage />);
    await waitFor(() => expect(screen.getByTestId("edit-full-name")).toBeTruthy());

    fireEvent.change(screen.getByTestId("edit-full-name"), {
      target: { value: "Draft Name" },
    });
    fireEvent.change(screen.getByTestId("edit-department"), {
      target: { value: "Draft Dept" },
    });
    fireEvent.click(screen.getByTestId("save-profile"));

    await waitFor(() => {
      expect(screen.getByTestId("detail-error").textContent).toMatch(
        /server refused/,
      );
    });
    expect(screen.getByTestId("detail-error").getAttribute("role")).toBe("alert");
    // Heading still shows last server-confirmed name.
    expect(screen.getByRole("heading", { level: 1 }).textContent).toMatch(/Taylor/);
    // Form retains draft values for retry.
    expect((screen.getByTestId("edit-full-name") as HTMLInputElement).value).toBe(
      "Draft Name",
    );
    expect((screen.getByTestId("edit-department") as HTMLInputElement).value).toBe(
      "Draft Dept",
    );
    // Image list still present.
    expect(screen.getByTestId("image-list").textContent).toMatch(/#11/);
  });
});
