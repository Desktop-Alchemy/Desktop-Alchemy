<script setup lang="ts">
import { t } from "../i18n";

import { computed, onMounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useDesktopAlchemy } from "../store";
import type { ImageRef, LibraryEdit } from "../bindings";
import AppDialog from "./AppDialog.vue";
import AssetThumbnail from "./AssetThumbnail.vue";
import AppSelect from "./AppSelect.vue";
import AppIcon from "./AppIcon.vue";
const props = defineProps<{ pick?: boolean; staticOnly?: boolean }>();
const emit = defineEmits<{ select: [image: ImageRef] }>();
const w = useDesktopAlchemy();
const current = ref<string | null>(null);
const query = ref("");
const currentPlugin = computed(
  () => w.library.folders.find((f) => f.id === current.value)?.plugin_id,
);
const selectedFolders = ref<string[]>([]);
const selectedItems = ref<string[]>([]);
const dialog = ref<"folder" | "rename" | "move" | "delete" | null>(null);
const inputName = ref("");
const moveTarget = ref<string | null>(null);
const results = ref<{ path: string; error: string | null }[]>([]);
const folders = computed(() =>
  w.library.folders.filter((f) => f.parent_id === current.value),
);
const items = computed(() =>
  w.library.items.filter(
    (i) =>
      i.folder_id === current.value &&
      (!props.staticOnly ||
        i.image.type !== "asset" ||
        !i.image.asset_id.endsWith(".gif")) &&
      i.name
        .toLocaleLowerCase()
        .includes(query.value.trim().toLocaleLowerCase()),
  ),
);
const count = computed(
  () => selectedFolders.value.length + selectedItems.value.length,
);
const tree = computed(() => {
  const result: {
    id: string;
    name: string;
    depth: number;
    path: string;
    plugin_id: string | null;
  }[] = [];
  function visit(parent: string | null, depth: number, prefix: string) {
    for (const f of w.library.folders.filter((f) => f.parent_id === parent)) {
      const path = prefix + f.name;
      result.push({
        id: f.id,
        name: f.name,
        depth,
        path,
        plugin_id: f.plugin_id,
      });
      visit(f.id, depth + 1, path + " / ");
    }
  }
  visit(null, 0, "");
  return result;
});
const breadcrumbs = computed(() => {
  const result = [];
  let id = current.value;
  while (id) {
    const folder = w.library.folders.find((f) => f.id === id);
    if (!folder) break;
    result.unshift(folder);
    id = folder.parent_id;
  }
  return result;
});
const affected = computed(() => {
  const ids = new Set(selectedFolders.value);
  for (const f of tree.value) {
    const parent = w.library.folders.find((x) => x.id === f.id)?.parent_id;
    if (parent && ids.has(parent)) ids.add(f.id);
  }
  return {
    folders: ids.size,
    items: w.library.items.filter(
      (i) =>
        selectedItems.value.includes(i.id) ||
        (i.folder_id && ids.has(i.folder_id)),
    ).length,
  };
});
const moveOptions = computed(() =>
  tree.value.filter((f) => {
    if (f.plugin_id) return false;
    let id: string | null = f.id;
    while (id) {
      if (selectedFolders.value.includes(id)) return false;
      id = w.library.folders.find((x) => x.id === id)?.parent_id ?? null;
    }
    return true;
  }),
);
watch(current, () => {
  query.value = "";
  selectedFolders.value = [];
  selectedItems.value = [];
});
watch(
  () => w.library,
  () => {
    if (current.value && !w.library.folders.some((f) => f.id === current.value))
      current.value = null;
    selectedFolders.value = selectedFolders.value.filter((id) =>
      w.library.folders.some((f) => f.id === id),
    );
    selectedItems.value = selectedItems.value.filter((id) =>
      w.library.items.some((i) => i.id === id),
    );
  },
);
let editBase: string | undefined;
function begin(kind: typeof dialog.value) {
  editBase = w.library.personal_revision;
  w.error = "";
  inputName.value =
    kind === "rename"
      ? (w.library.folders.find((f) => f.id === selectedFolders.value[0])
          ?.name ??
        w.library.items.find((i) => i.id === selectedItems.value[0])?.name ??
        "")
      : "";
  moveTarget.value = null;
  dialog.value = kind;
}
async function commit() {
  let edit: LibraryEdit;
  if (dialog.value === "folder")
    edit = {
      type: "create_folder",
      parent_id: current.value,
      name: inputName.value.trim(),
    };
  else if (dialog.value === "rename")
    edit = selectedFolders.value.length
      ? {
          type: "rename_folder",
          id: selectedFolders.value[0],
          name: inputName.value.trim(),
        }
      : {
          type: "rename_item",
          id: selectedItems.value[0],
          name: inputName.value.trim(),
        };
  else if (dialog.value === "move")
    edit = {
      type: "move",
      folder_ids: selectedFolders.value,
      item_ids: selectedItems.value,
      parent_id: moveTarget.value,
    };
  else
    edit = {
      type: "delete",
      folder_ids: selectedFolders.value,
      item_ids: selectedItems.value,
    };
  if (await w.editLibrary(edit, editBase)) {
    dialog.value = null;
    selectedFolders.value = [];
    selectedItems.value = [];
  }
}
async function importImages() {
  const paths = await open({
    multiple: true,
    filters: [
      {
        name: t("ui.imageAssets"),
        extensions: props.staticOnly
          ? ["png", "jpg", "jpeg"]
          : ["png", "jpg", "jpeg", "gif"],
      },
    ],
  });
  if (!paths) return;
  const imported = await w.importLibrary(
    typeof paths === "string" ? [paths] : paths,
    current.value,
  );
  results.value = imported ?? [];
}
onMounted(() => void w.loadLibrary());
</script>
<template>
  <div class="library-browser">
    <div class="library-tools button-row">
      <button
        class="primary"
        :disabled="
          !w.desktop || !!w.libraryError || w.pending > 0 || !!currentPlugin
        "
        @click="importImages"
      >
        {{ t("ui.importImages") }}
      </button>
      <template v-if="!pick">
        <button
          :disabled="
            !w.desktop || !!w.libraryError || w.pending > 0 || !!currentPlugin
          "
          @click="begin('folder')"
        >
          {{ t("ui.newFolder") }}
        </button>
        <button
          :disabled="count !== 1 || w.pending > 0"
          @click="begin('rename')"
        >
          {{ t("ui.rename") }}
        </button>
        <button :disabled="!count || w.pending > 0" @click="begin('move')">
          {{ t("ui.move") }}
        </button>
        <button
          class="danger"
          :disabled="!count || w.pending > 0"
          @click="begin('delete')"
        >
          {{ t("ui.delete2") }}
        </button>
        <span v-if="count" class="helper">{{
          t("phrases.selectedCount", { count })
        }}</span>
      </template>
    </div>
    <label class="library-search"
      >{{ t("ui.searchImages")
      }}<input
        v-model="query"
        type="search"
        :placeholder="t('ui.searchThisFolderByName')"
    /></label>
    <p v-if="currentPlugin" class="helper">
      {{ t("ui.pluginIcons") }} {{ currentPlugin }}
      {{ t("ui.managedByPlugin") }}
    </p>
    <div v-if="w.libraryError" class="notice">
      <p role="alert">{{ w.libraryError }}</p>
      <button @click="w.loadLibrary">{{ t("ui.retry") }}</button>
    </div>
    <details
      v-if="results.length"
      class="import-results"
      :open="results.some((r) => r.error)"
    >
      <summary>
        {{
          t("phrases.importSummary", {
            success: results.filter((r) => !r.error).length,
            failed: results.filter((r) => r.error).length,
          })
        }}
      </summary>
      <p
        v-for="(result, index) in results"
        :key="index"
        :class="{ 'error-text': result.error }"
      >
        {{ result.path.split(/[\\/]/).at(-1) }}:
        {{ result.error ? t("phrases.assetImportFailed") : t("ui.added") }}
        <details v-if="result.error">
          <summary>{{ t("errors.details") }}</summary>
          {{ result.error }}
        </details>
      </p>
    </details>
    <div v-if="!w.libraryError" class="library-layout">
      <aside class="library-tree" :aria-label="t('ui.assetFolders')">
        <button :class="{ active: current === null }" @click="current = null">
          {{ t("ui.imageAssets") }}
        </button>
        <button
          v-for="f in tree"
          :key="f.id"
          :class="{ active: current === f.id }"
          :style="{ paddingLeft: `${12 + f.depth * 14}px` }"
          :title="f.plugin_id ? `${f.path} · ${f.plugin_id}` : f.path"
          @click="current = f.id"
        >
          <AppIcon name="folder" :size="16" /> {{ f.name
          }}<small v-if="f.plugin_id" class="tag">{{ t("ui.plugin") }}</small>
        </button>
      </aside>
      <section class="library-main">
        <nav class="library-breadcrumb" :aria-label="t('ui.folderPath')">
          <button class="quiet" @click="current = null">
            {{ t("ui.imageAssets") }}</button
          ><template v-for="f in breadcrumbs" :key="f.id"
            ><span>/</span
            ><button class="quiet" @click="current = f.id">
              {{ f.name }}
            </button></template
          >
        </nav>
        <p v-if="!folders.length && !items.length" class="library-empty">
          {{
            staticOnly
              ? t("ui.noStaticImagesInThisFolder")
              : currentPlugin
                ? t("ui.noMatchingIconsInThisPlugin")
                : t("ui.noImagesInThisFolderClick")
          }}
        </p>
        <div class="library-grid">
          <div
            v-for="folder in folders"
            :key="folder.id"
            class="library-tile"
            :class="{ selected: selectedFolders.includes(folder.id) }"
          >
            <input
              v-if="!pick && !folder.plugin_id"
              v-model="selectedFolders"
              type="checkbox"
              :value="folder.id"
              :aria-label="t('ui.selectFolder', { arg0: folder.name })"
            />
            <button
              class="library-tile-button"
              :title="t('ui.openFolder', { arg0: folder.name })"
              :aria-label="t('ui.openFolder', { arg0: folder.name })"
              @click="current = folder.id"
            >
              <span class="folder-symbol"
                ><AppIcon name="folder" :size="36" weight="duotone" /></span
              ><span class="library-name">{{ folder.name }}</span
              ><small v-if="folder.plugin_id" class="tag">{{
                t("ui.pluginIcons2")
              }}</small>
            </button>
          </div>
          <div
            v-for="item in items"
            :key="item.id"
            class="library-tile"
            :class="{ selected: selectedItems.includes(item.id) }"
          >
            <input
              v-if="!pick && item.image.type === 'asset'"
              v-model="selectedItems"
              type="checkbox"
              :value="item.id"
              :aria-label="t('ui.selectAsset', { arg0: item.name })"
            />
            <button
              class="library-tile-button"
              :title="pick ? t('ui.use2', { arg0: item.name }) : item.name"
              :aria-label="pick ? t('ui.use2', { arg0: item.name }) : item.name"
              @click="
                pick
                  ? emit('select', item.image)
                  : item.image.type === 'asset' &&
                    (selectedItems = selectedItems.includes(item.id)
                      ? selectedItems.filter((id) => id !== item.id)
                      : [...selectedItems, item.id])
              "
            >
              <AssetThumbnail
                :image="item.image"
                :revision="item.revision"
                :name="item.name"
              /><span class="library-name">{{ item.name }}</span>
            </button>
          </div>
        </div>
      </section>
    </div>
    <AppDialog
      v-if="dialog"
      :title="
        {
          folder: t('ui.newFolder'),
          rename: t('ui.rename'),
          move: t('ui.moveSelectedItems'),
          delete: t('ui.deleteSelectedItems'),
        }[dialog]
      "
      @close="dialog = null"
    >
      <form @submit.prevent="commit">
        <label v-if="dialog === 'folder' || dialog === 'rename'"
          >{{ t("ui.name")
          }}<input v-model="inputName" required maxlength="120"
        /></label>
        <label v-if="dialog === 'move'"
          >{{ t("ui.destinationFolder")
          }}<AppSelect
            v-model="moveTarget"
            :aria-label="t('ui.destinationFolder')"
            :options="[
              { value: null, label: t('ui.imageAssetsRoot') },
              ...moveOptions.map((f) => ({ value: f.id, label: f.path })),
            ]"
        /></label>
        <p v-if="dialog === 'delete'">
          {{
            t("phrases.deleteAssets", {
              folders: affected.folders,
              items: affected.items,
            })
          }}
        </p>
        <p v-if="w.error" class="error-text" role="alert">{{ w.error }}</p>
        <div class="button-row dialog-actions">
          <button type="button" data-cancel @click="dialog = null">
            {{ t("ui.cancel") }}</button
          ><button class="primary" :disabled="w.pending > 0">
            {{ dialog === "delete" ? t("ui.delete2") : t("ui.confirm") }}
          </button>
        </div>
      </form>
    </AppDialog>
  </div>
</template>
