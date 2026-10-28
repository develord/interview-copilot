import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { SESSION_CONTROLLER, type SessionController } from "@/services/sessionController";
import OverlayFooter from "./OverlayFooter.vue";

function mountFooter() {
  const controller = { askManual: vi.fn() } as unknown as SessionController;
  const wrapper = mount(OverlayFooter, {
    attachTo: document.body,
    global: { provide: { [SESSION_CONTROLLER as symbol]: controller } },
  });
  return { wrapper, controller };
}

describe("OverlayFooter", () => {
  const escapes = vi.fn();
  const onWindowKeydown = (e: KeyboardEvent) => e.key === "Escape" && escapes();

  afterEach(() => {
    window.removeEventListener("keydown", onWindowKeydown);
    escapes.mockReset();
    document.body.innerHTML = "";
  });

  // Regression: Escape in an empty input was swallowed, so overlay panels could not be closed.
  it("lets Escape reach the overlay when the input is empty", async () => {
    window.addEventListener("keydown", onWindowKeydown);
    const { wrapper } = mountFooter();

    await wrapper.get("input").trigger("keydown", { key: "Escape" });

    expect(escapes).toHaveBeenCalledTimes(1);
  });

  it("uses the first Escape to clear a draft without closing panels", async () => {
    window.addEventListener("keydown", onWindowKeydown);
    const { wrapper } = mountFooter();
    const input = wrapper.get("input");
    await input.setValue("half-typed question");

    await input.trigger("keydown", { key: "Escape" });

    expect((input.element as HTMLInputElement).value).toBe("");
    expect(escapes).not.toHaveBeenCalled();
  });

  it("submits a trimmed manual question and resets the field", async () => {
    const { wrapper, controller } = mountFooter();
    const input = wrapper.get("input");
    await input.setValue("  What is a closure?  ");

    await wrapper.get("form").trigger("submit");

    expect(controller.askManual).toHaveBeenCalledWith("What is a closure?");
    expect((input.element as HTMLInputElement).value).toBe("");
  });

  it("requires a second click to clear the history", async () => {
    const { wrapper } = mountFooter();
    const clearButton = wrapper.findAll("button").at(-1)!;

    await clearButton.trigger("click");
    expect(wrapper.emitted("clear")).toBeUndefined();
    await clearButton.trigger("click");
    expect(wrapper.emitted("clear")).toHaveLength(1);
  });
});
