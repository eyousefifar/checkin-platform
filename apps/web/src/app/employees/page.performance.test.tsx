import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Employee } from "@/lib/types";

const apiMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  api: (...args: unknown[]) => apiMock(...args),
  API_URL: "http://localhost:8000",
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

import EmployeesPage from "./page";

const employees: Employee[] = [
  {
    id: 1,
    employee_code: "E100",
    full_name: "Alice Zeta",
    department: "Finance",
    is_active: true,
    image_count: 1,
    usable_images: 1,
    embedding_ready: true,
    num_images_used: 1,
  },
  {
    id: 2,
    employee_code: "E200",
    full_name: "Bob Alpha",
    department: "Ops",
    is_active: true,
    image_count: 0,
    usable_images: 0,
    embedding_ready: false,
    num_images_used: 0,
  },
  {
    id: 3,
    employee_code: "X9",
    full_name: "Cara Beta",
    department: "Finance",
    is_active: false,
    image_count: 2,
    usable_images: 1,
    embedding_ready: true,
    num_images_used: 1,
  },
];

describe("EmployeesPage local filter performance", () => {
  beforeEach(() => {
    apiMock.mockReset();
    apiMock.mockResolvedValue(employees);
  });

  it("fetches once and filters code/name/department locally with stable order", async () => {
    render(<EmployeesPage />);
    await waitFor(() => {
      expect(screen.getAllByTestId("employee-row")).toHaveLength(3);
    });
    expect(apiMock).toHaveBeenCalledTimes(1);
    expect(apiMock.mock.calls[0][0]).toBe("/api/employees");

    const search = screen.getByTestId("employee-search");
    // Multiple keystrokes must not issue more network requests.
    fireEvent.change(search, { target: { value: "f" } });
    fireEvent.change(search, { target: { value: "fi" } });
    fireEvent.change(search, { target: { value: "fin" } });
    fireEvent.change(search, { target: { value: "finance" } });

    expect(apiMock).toHaveBeenCalledTimes(1);
    const financeRows = screen.getAllByTestId("employee-row");
    // Alice then Cara (server order preserved; both Finance).
    expect(financeRows).toHaveLength(2);
    expect(financeRows[0].textContent).toMatch(/Alice Zeta/);
    expect(financeRows[1].textContent).toMatch(/Cara Beta/);

    fireEvent.change(search, { target: { value: "e200" } });
    expect(screen.getAllByTestId("employee-row")).toHaveLength(1);
    expect(screen.getByTestId("employee-row").textContent).toMatch(/Bob Alpha/);

    fireEvent.change(search, { target: { value: "alpha" } });
    expect(screen.getAllByTestId("employee-row")).toHaveLength(1);
    expect(screen.getByTestId("employee-row").textContent).toMatch(/Bob Alpha/);

    fireEvent.change(search, { target: { value: "" } });
    expect(screen.getAllByTestId("employee-row")).toHaveLength(3);
    expect(apiMock).toHaveBeenCalledTimes(1);
  });

  it("shows load error without inventing rows", async () => {
    apiMock.mockRejectedValueOnce(new Error("unauthorized"));
    render(<EmployeesPage />);
    await waitFor(() => {
      expect(screen.getByText(/unauthorized/i)).toBeTruthy();
    });
    expect(screen.queryAllByTestId("employee-row")).toHaveLength(0);
  });
});
