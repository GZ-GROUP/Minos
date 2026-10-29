import {
  File,
  FileAudio,
  FileCode,
  FileCsv,
  FileDoc,
  FileImage,
  FilePdf,
  FilePpt,
  FileText,
  FileVideo,
  FileXls,
  FileZip,
  Folder,
  type Icon,
} from "@phosphor-icons/react";

const GROUPS: [Icon, string[]][] = [
  [FileImage, ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "heic", "tif", "tiff"]],
  [FilePdf, ["pdf"]],
  [FileDoc, ["doc", "docx", "odt", "rtf"]],
  [FileXls, ["xls", "xlsx", "ods"]],
  [FileCsv, ["csv", "tsv"]],
  [FilePpt, ["ppt", "pptx", "odp"]],
  [FileZip, ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "tgz"]],
  [FileAudio, ["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus"]],
  [FileVideo, ["mp4", "mkv", "mov", "avi", "webm", "wmv"]],
  [
    FileCode,
    ["js", "jsx", "ts", "tsx", "json", "rs", "py", "java", "c", "h", "cpp", "hpp", "cs", "go", "rb", "php", "html", "css", "scss", "sh", "ps1", "sql", "xml"],
  ],
  [FileText, ["txt", "md", "log", "ini", "cfg", "conf", "yaml", "yml", "toml", "env"]],
];

const BY_EXTENSION = new Map(GROUPS.flatMap(([icon, extensions]) => extensions.map((ext) => [ext, icon] as const)));

/** Icon for a file-system entry, picked by extension. */
export function fileIcon(name: string, kind: "folder" | "file"): Icon {
  if (kind === "folder") return Folder;
  const dot = name.lastIndexOf(".");
  const extension = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
  return BY_EXTENSION.get(extension) ?? File;
}
