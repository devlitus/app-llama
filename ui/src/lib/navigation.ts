export const SECTIONS = [
  { id: "explore", label: "Explorar" },
  { id: "library", label: "Biblioteca" },
  { id: "run", label: "Ejecutar" },
  { id: "chat", label: "Chat" },
  { id: "settings", label: "Ajustes" },
] as const;

export type SectionId = (typeof SECTIONS)[number]["id"];

export const INITIAL_SECTION: SectionId = "explore";

export function isSectionId(value: string): value is SectionId {
  return SECTIONS.some((section) => section.id === value);
}

export function sectionLabel(id: SectionId): string {
  const section = SECTIONS.find((candidate) => candidate.id === id);
  return section ? section.label : id;
}
