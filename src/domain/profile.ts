import { z } from "zod";

/**
 * The verified profile, the only source of personal facts in answers. Mirrors
 * `src-tauri/src/profile/mod.rs`. A draft extracted from a CV has `verifiedAt: null` until the
 * user reviews and saves it.
 */

export const PROFILE_FIELDS = [
  "name",
  "headline",
  "summary",
  "skills",
  "experience",
  "projects",
  "education",
  "languages",
  "targetRole",
] as const;
export type ProfileField = (typeof PROFILE_FIELDS)[number];

export const MAX_FIELD_CHARS = 8000;
export const MAX_PDF_BYTES = 10 * 1024 * 1024;

const field = z.string().max(MAX_FIELD_CHARS);

export const profileSchema = z.object({
  name: field,
  headline: field,
  summary: field,
  skills: field,
  experience: field,
  projects: field,
  education: field,
  languages: field,
  targetRole: field,
  verifiedAt: z.number().int().nonnegative().nullable(),
});
export type Profile = z.infer<typeof profileSchema>;

export const PROFILE_ERROR_CODES = ["invalid", "storage", "missing-key", "extraction"] as const;
export type ProfileErrorCode = (typeof PROFILE_ERROR_CODES)[number];

export const profileErrorSchema = z.object({
  code: z.enum(PROFILE_ERROR_CODES),
  detail: z.string().max(2000),
});
export type ProfileError = z.infer<typeof profileErrorSchema>;

export function emptyProfile(): Profile {
  return {
    name: "",
    headline: "",
    summary: "",
    skills: "",
    experience: "",
    projects: "",
    education: "",
    languages: "",
    targetRole: "",
    verifiedAt: null,
  };
}

export function asProfileError(value: unknown): ProfileError {
  const parsed = profileErrorSchema.safeParse(value);
  if (parsed.success) return parsed.data;
  return { code: "storage", detail: value instanceof Error ? value.message : String(value) };
}

/** Reads a dropped or picked file as base64 (no data-URL prefix), for `profile_extract_pdf`. */
export function readAsBase64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("cannot read the file"));
    reader.onload = () => {
      const url = String(reader.result);
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.readAsDataURL(file);
  });
}
