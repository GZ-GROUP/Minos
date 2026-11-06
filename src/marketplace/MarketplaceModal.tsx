import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Cpu,
  FileText,
  FolderOpen,
  MagnifyingGlass,
  Plus,
  ShareNetwork,
  SquaresFour,
  Storefront,
  Wrench,
  X,
  type Icon,
} from "@phosphor-icons/react";
import type { AppCategory, AppDefinition } from "../apps/types";
import { Button } from "../components/Button";
import { StateMessage } from "../components/StateMessage";
import { Dialog } from "../components/dialogs/Dialog";
import { useScrollEdges } from "../hooks/useScrollEdges";
import { AddModuleDialog } from "./AddModuleDialog";
import type { Localized } from "./api";
import { ModuleCard, type MarketplaceItem } from "./ModuleCard";
import type { Marketplace } from "./useMarketplace";
import "./Marketplace.css";

const CATEGORIES: { id: AppCategory; icon: Icon }[] = [
  { id: "documents", icon: FileText },
  { id: "files", icon: FolderOpen },
  { id: "system", icon: Cpu },
  { id: "network", icon: ShareNetwork },
  { id: "utilities", icon: Wrench },
];

const CATEGORY_ICONS = Object.fromEntries(CATEGORIES.map(({ id, icon }) => [id, icon])) as Record<AppCategory, Icon>;

type CategoryFilter = AppCategory | "all";

interface CategorizedItem extends MarketplaceItem {
  category: AppCategory;
}

function normalize(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase();
}

function repoName(repo: string): string {
  return repo.replace("https://github.com/", "");
}

interface MarketplaceModalProps {
  open: boolean;
  onClose: () => void;
  apps: AppDefinition[];
  market: Marketplace;
}

export function MarketplaceModal({ open, onClose, apps, market }: MarketplaceModalProps) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<CategoryFilter>("all");
  const [addOpen, setAddOpen] = useState(false);
  const { loadCatalog, catalog, data, loadError } = market;

  useEffect(() => {
    if (!open) return;
    loadCatalog();
    setQuery("");
    setCategory("all");
  }, [open, loadCatalog]);

  const items = useMemo<CategorizedItem[]>(() => {
    const pick = (values: Localized) => values[i18n.language] ?? values.en;

    const integrated: CategorizedItem[] = apps.map((app) => ({
      id: app.id,
      kind: "integrated",
      name: i18n.t("name", { ns: app.id }),
      description: i18n.t("description", { ns: app.id }),
      icon: app.icon,
      category: app.category,
      platforms: app.platforms,
      meta: [t("marketplace.included")],
      repo: null,
      local: false,
      downloaded: false,
    }));

    const local: CategorizedItem[] = (data?.modules ?? []).map(({ manifest, repo, downloaded }) => ({
      id: manifest.id,
      kind: "external",
      name: pick(manifest.name),
      description: pick(manifest.description),
      icon: CATEGORY_ICONS[manifest.category],
      category: manifest.category,
      platforms: manifest.platforms,
      meta: [manifest.author, `v${manifest.version}`, repo ? repoName(repo) : t("marketplace.localZip")],
      repo,
      local: true,
      downloaded,
    }));

    const localIds = new Set(local.map((item) => item.id));
    const offered: CategorizedItem[] = (catalog?.modules ?? [])
      .filter((entry) => !localIds.has(entry.id))
      .map((entry) => ({
        id: entry.id,
        kind: "external",
        name: pick(entry.name),
        description: pick(entry.description),
        icon: CATEGORY_ICONS[entry.category],
        category: entry.category,
        platforms: entry.platforms,
        meta: [entry.author, repoName(entry.repo)],
        repo: entry.repo,
        local: false,
        downloaded: false,
      }));

    return [...integrated, ...local, ...offered].sort((a, b) => a.name.localeCompare(b.name, i18n.language));
  }, [apps, data, catalog, i18n, i18n.language, t]);

  const matching = useMemo(() => {
    const needle = normalize(query.trim());
    if (!needle) return items;
    return items.filter((item) => normalize(`${item.name} ${item.description} ${item.meta.join(" ")}`).includes(needle));
  }, [items, query]);

  const visible = category === "all" ? matching : matching.filter((item) => item.category === category);
  const { ref: listRef, update: updateEdges, fadeProps } = useScrollEdges<HTMLUListElement>([visible.length]);

  const filters: { id: CategoryFilter; icon: Icon; count: number }[] = [
    { id: "all", icon: SquaresFour, count: matching.length },
    ...CATEGORIES.map(({ id, icon }) => ({
      id,
      icon,
      count: matching.filter((item) => item.category === id).length,
    })),
  ];

  return (
    <>
      <Dialog
        open={open}
        onClose={onClose}
        title={t("marketplace.title")}
        icon={Storefront}
        size="wide"
        headerEnd={
          <>
            <label className="marketplace__search">
              <MagnifyingGlass aria-hidden="true" />
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t("marketplace.search")}
                aria-label={t("marketplace.search")}
                autoComplete="off"
                spellCheck={false}
              />
            </label>
            <Button icon={Plus} disabled={!data} onClick={() => setAddOpen(true)}>
              {t("marketplace.add.open")}
            </Button>
            <Button icon={X} iconOnly variant="ghost" label={t("marketplace.close")} onClick={onClose} />
          </>
        }
      >
        {open && (
          <div className="marketplace">
            <nav className="marketplace__categories" aria-label={t("marketplace.categoriesLabel")}>
              {filters.map(({ id, icon: FilterIcon, count }) => (
                <button
                  key={id}
                  type="button"
                  className="marketplace__category"
                  aria-current={category === id ? "true" : undefined}
                  onClick={() => setCategory(id)}
                >
                  <FilterIcon aria-hidden="true" />
                  <span>{t(`marketplace.categories.${id}`)}</span>
                  <span className="marketplace__count">{count}</span>
                </button>
              ))}
            </nav>

            <div className="marketplace__content">
              {catalog?.offline && <p className="marketplace__notice">{t("marketplace.offline")}</p>}
              {!data && loadError ? (
                <StateMessage kind="error" error={loadError} />
              ) : visible.length > 0 ? (
                <ul ref={listRef} className="marketplace__list scroll-fade" onScroll={updateEdges} {...fadeProps}>
                  {visible.map((item) => (
                    <ModuleCard key={item.id} item={item} market={market} />
                  ))}
                </ul>
              ) : (
                <StateMessage
                  kind="empty"
                  message={query.trim() ? t("list.noResults", { query: query.trim() }) : t("marketplace.emptyCategory")}
                />
              )}
            </div>
          </div>
        )}
      </Dialog>

      <AddModuleDialog open={addOpen} onClose={() => setAddOpen(false)} market={market} />
    </>
  );
}
