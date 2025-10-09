import { RefCallback, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { NodeApi, NodeRendererProps, Tree, TreeApi } from "react-arborist";
import useResizeObserver from "use-resize-observer";
import RunbookTreeRow, { RunbookRowData, RunbookTreeRowProps } from "./TreeView/RunbookTreeRow";
import FolderTreeRow, { FolderRowData, FolderTreeRowProps } from "./TreeView/FolderTreeRow";
import { useDragDropManager } from "react-dnd";
import debounce from "lodash.debounce";
import { useStore } from "@/state/store";
import { actions } from "react-arborist/dist/module/state/dnd-slice";

export type TreeRowData = FolderRowData | RunbookRowData;

export enum SortBy {
  Name = "name",
  NameAsc = "name_asc",
  Updated = "updated",
  UpdatedAsc = "updated_asc",
}

interface MoveHandlerArgs<T> {
  dragIds: string[];
  dragNodes: NodeApi<T>[];
  parentId: string | null;
  parentNode: NodeApi<T> | null;
  index: number;
}

interface TreeViewProps {
  data: TreeRowData[];
  workspaceId?: string;
  workspaceOnline: boolean;
  workspaceLegacyHybrid: boolean;
  sortBy: SortBy;
  selectedItemId: string | null;
  initialOpenState: Record<string, boolean>;
  onTreeApiReady: (treeApi: TreeApi<TreeRowData>) => void;
  onActivateItem: (itemId: string) => void;
  onNewFolder: (parentId: string | null) => void;
  onRenameFolder: (folderId: string, newName: string) => void;
  onMoveItems: (
    ids: string[],
    sourceWorkspaceId: string,
    parentId: string | null,
    index: number,
  ) => void;
  onContextMenu: (evt: React.MouseEvent<HTMLDivElement>, itemId: string) => void;
  onToggleFolder: (nodeId: string) => void;
}

export default function TreeView(props: TreeViewProps) {
  const { ref: resizeRef, width } = useResizeObserver();
  const [divElement, setDivElement] = useState<HTMLDivElement | null>(null);
  const treeRef = useRef<TreeApi<TreeRowData> | null>(null);
  const lastSidebarDragInfo = useStore((state) => state.lastSidebarDragInfo);

  // See https://github.com/brimdata/react-arborist/issues/230#issuecomment-2404208311
  const dragDropManager = useDragDropManager();

  function handleActivate(node: NodeApi<TreeRowData>) {
    props.onActivateItem(node.data.id); // TODO: rename
  }

  function handleSelect(nodes: NodeApi<TreeRowData>[]) {
    const tree = treeRef.current;
    if (!tree) {
      return;
    }

    const depths = new Set(nodes.map((n) => n.level));
    const types = new Set(nodes.map((n) => n.data.type));
    // If we're selecting nodes of different types AND different depths,
    // only select nodes from the same depth as the original selection.
    if (depths.size > 1 && types.size > 1) {
      const selectedNodes = treeRef.current!.selectedNodes;
      const depth = selectedNodes[selectedNodes.length - 1]!.level;
      for (const node of nodes) {
        if (node.level !== depth) {
          tree.deselect(node.data.id);
        }
      }
    }
  }

  const [rowCount, setRowCount] = useState<number | null>(null);

  const mouseMoveCallback = useMemo(() => {
    return debounce(() => {
      const monitor = dragDropManager.getMonitor();
      if (monitor.isDragging()) {
        const item = monitor.getItem();
        const { setLastSidebarDragInfo } = useStore.getState();
        if (item) {
          setLastSidebarDragInfo({
            itemIds: item.dragIds,
            sourceWorkspaceId: props.workspaceId!,
          });
        }
      }
    }, 100);
  }, []);

  // Both the resize observer and the mouse move callback need to be
  // attached to the same element, so we use a composite ref.
  const compositeRef: RefCallback<HTMLDivElement> = useCallback((el) => {
    if (el) {
      resizeRef(el);
      setDivElement(el);
    }
  }, []);

  useEffect(() => {
    if (divElement) {
      divElement.addEventListener("mousemove", mouseMoveCallback);
    }
    return () => {
      if (divElement) {
        divElement.removeEventListener("mousemove", mouseMoveCallback);
      }
    };
  }, [divElement]);

  function handleToggle(nodeId: string) {
    setTimeout(() => setRowCount(treeRef.current?.visibleNodes.length || 10));
    props.onToggleFolder(nodeId);
  }

  function handleMove(args: MoveHandlerArgs<TreeRowData>) {
    const { lastSidebarDragInfo, setLastSidebarDragInfo } = useStore.getState();
    setLastSidebarDragInfo(undefined);

    let ids = args.dragIds;
    let sourceWorkspaceId = props.workspaceId;
    const parentId = args.parentId;
    const index = args.index;

    // If the dragIds are empty, it means we've dragged items from some other workspace into this workspace
    // (which isn't supported natively by react-arborist). The source workspace and dragged item IDs are
    // stored in the state via a manual mouse-move handler (below).
    if (args.dragIds.length == 0) {
      if (lastSidebarDragInfo) {
        ids = lastSidebarDragInfo.itemIds;
        sourceWorkspaceId = lastSidebarDragInfo.sourceWorkspaceId;
      }
    }

    if (ids.length > 0 && sourceWorkspaceId) {
      // HACK [mkt]: We need to manually dispatch a dragEnd to stop the drag indicator from showing.
      treeRef.current?.store.dispatch(actions.dragEnd());
      props.onMoveItems(ids, sourceWorkspaceId, parentId, index);
    }
  }

  // Hack to allow TreeRow to be memoized, which keeps
  // the inline editor for resetting during sync
  const onContextMenuRef = useRef(props.onContextMenu);
  useEffect(() => {
    onContextMenuRef.current = props.onContextMenu;
  }, [props.onContextMenu]);
  const onContextMenu = useCallback((evt: React.MouseEvent<HTMLDivElement>, itemId: string) => {
    onContextMenuRef.current(evt, itemId);
  }, []);

  const TreeRow = useMemo(
    () => (innerProps: NodeRendererProps<FolderRowData | RunbookRowData>) => {
      if (innerProps.node.data.type === "folder") {
        const folderProps: FolderTreeRowProps = {
          style: innerProps.style,
          node: innerProps.node as NodeApi<FolderRowData>,
          tree: innerProps.tree as TreeApi<FolderRowData>,
          dragHandle: innerProps.dragHandle,
          preview: innerProps.preview,
          onContextMenu,
        };
        return <FolderTreeRow key={innerProps.node.data.id} {...folderProps} />;
      } else {
        const runbookProps: RunbookTreeRowProps = {
          style: innerProps.style,
          node: innerProps.node as NodeApi<RunbookRowData>,
          tree: innerProps.tree as TreeApi<RunbookRowData>,
          dragHandle: innerProps.dragHandle,
          preview: innerProps.preview,
          runbookId: innerProps.node.data.id,
          onContextMenu,
          useProvidedName: !props.workspaceOnline && !props.workspaceLegacyHybrid,
        };
        return <RunbookTreeRow key={innerProps.node.data.id} {...runbookProps} />;
      }
    },
    [onContextMenu],
  );

  async function handleRename({ id, name }: { id: string; name: string }) {
    props.onRenameFolder(id, name);
  }

  const ref = useCallback((tree?: TreeApi<TreeRowData> | null) => {
    if (tree) {
      treeRef.current = tree;
      props.onTreeApiReady(tree);
      setTimeout(() => setRowCount(treeRef.current?.visibleNodes.length || 1), 25);
    }
  }, []);

  const height = useMemo(() => {
    return Math.max((rowCount || 1) * 24 + 8, 10);
  }, [rowCount]);

  return (
    <div ref={compositeRef} className="w-[98%] m-auto">
      <Tree
        ref={ref}
        data={props.data}
        // By not setting the selection until we know the tree height, we avoid a bug
        // where the tree would sometimes not render the first few rows after trying to
        // scroll to the selected node while the tree was resizing.
        selection={rowCount ? props.selectedItemId ?? undefined : undefined}
        openByDefault={true}
        initialOpenState={props.initialOpenState}
        width={width}
        height={height}
        padding={4}
        onActivate={handleActivate}
        onMove={handleMove}
        onSelect={handleSelect}
        onRename={handleRename}
        onToggle={handleToggle}
        dndManager={dragDropManager}
        disableDrag={props.workspaceLegacyHybrid}
        disableDrop={
          props.workspaceLegacyHybrid ||
          lastSidebarDragInfo?.sourceWorkspaceId !== props.workspaceId
        }
        disableEdit={props.workspaceLegacyHybrid}
        disableMultiSelection={props.workspaceLegacyHybrid}
      >
        {TreeRow}
      </Tree>
    </div>
  );
}
