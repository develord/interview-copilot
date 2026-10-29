import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia, type Pinia } from "pinia";
import { emptyProfile, type Profile } from "@/domain/profile";
import AnswerSettings from "./AnswerSettings.vue";

const backend = vi.hoisted(() => ({
  get: vi.fn<() => Promise<Profile | null>>(async () => null),
  extractPdf: vi.fn<(pdf: string) => Promise<Profile>>(),
  save: vi.fn<(p: Profile) => Promise<Profile>>(async (p) => ({ ...p, verifiedAt: 1 })),
  clear: vi.fn(async () => {}),
}));
vi.mock("@/platform/answerBackend", () => ({ profileBackend: backend }));

let pinia: Pinia;

function pdf(name = "cv.pdf", type = "application/pdf") {
  return new File(["%PDF-1.7 test"], name, { type });
}

describe("AnswerSettings", () => {
  beforeEach(() => {
    localStorage.clear();
    pinia = createPinia();
    setActivePinia(pinia);
    Object.values(backend).forEach((fn) => fn.mockClear());
  });

  it("turns a dropped CV into a draft that is only saved once reviewed", async () => {
    backend.extractPdf.mockResolvedValueOnce({
      ...emptyProfile(),
      name: "Alex Martin",
      skills: "Vue.js",
    });
    const wrapper = mount(AnswerSettings, { global: { plugins: [pinia] } });
    await flushPromises();

    await wrapper.find(".drop").trigger("drop", { dataTransfer: { files: [pdf()] } });
    await vi.waitFor(() => expect(backend.extractPdf).toHaveBeenCalled());
    await flushPromises();
    // The base64 of "%PDF" starts with JVBERi.
    expect(backend.extractPdf.mock.calls[0]![0].startsWith("JVBERi")).toBe(true);
    expect(backend.save).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("rien n'est utilisé avant ta validation");

    const name = wrapper.findAll("textarea")[0]!;
    expect((name.element as HTMLTextAreaElement).value).toBe("Alex Martin");
    await name.setValue("Alex B. Martin");
    await wrapper
      .findAll("button")
      .find((b) => b.text() === "Valider le profil")!
      .trigger("click");
    await flushPromises();

    expect(backend.save).toHaveBeenCalledWith(
      expect.objectContaining({ name: "Alex B. Martin", skills: "Vue.js", verifiedAt: null }),
    );
    expect(wrapper.text()).toContain("Alex B. Martin");
    expect(wrapper.text()).toContain("Profil validé");
  });

  it("refuses files that are not PDFs without calling Claude", async () => {
    const wrapper = mount(AnswerSettings, { global: { plugins: [pinia] } });
    await flushPromises();
    await wrapper
      .find(".drop")
      .trigger("drop", { dataTransfer: { files: [pdf("photo.png", "image/png")] } });
    await flushPromises();
    expect(backend.extractPdf).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("n'est pas un PDF");
  });

  it("asks for confirmation before deleting the profile", async () => {
    backend.get.mockResolvedValueOnce({ ...emptyProfile(), name: "Alex", verifiedAt: 5 });
    const wrapper = mount(AnswerSettings, { global: { plugins: [pinia] } });
    await flushPromises();
    const remove = () =>
      wrapper
        .findAll("button")
        .find((b) => b.text().includes("Supprimer") || b.text().includes("Confirmer"))!;
    await remove().trigger("click");
    expect(backend.clear).not.toHaveBeenCalled();
    await remove().trigger("click");
    await flushPromises();
    expect(backend.clear).toHaveBeenCalledTimes(1);
    expect(wrapper.text()).toContain("Profil supprimé");
  });
});
