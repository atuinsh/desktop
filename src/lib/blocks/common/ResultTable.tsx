import { AgGridReact } from "ag-grid-react";
import {
  ColDef,
  ModuleRegistry,
  AllCommunityModule,
  themeQuartz,
  colorSchemeLightWarm,
  colorSchemeDarkWarm,
  GridApi,
  CellDoubleClickedEvent,
  ColumnResizedEvent,
  GridReadyEvent,
} from "ag-grid-community";
import { useMemo, useRef, useEffect, useState } from "react";
import { useStore } from "@/state/store";
import { createPortal } from "react-dom";

// Register AG Grid modules
ModuleRegistry.registerModules([AllCommunityModule]);

interface ResultTableProps {
  columns: { id: string; title: string; grow?: number; width?: number }[];
  results: any[];
  setColumns?: (columns: { id: string; title: string; grow?: number; width?: number }[]) => void;
  width: string;
}

export default function ResultTable({ columns, results, setColumns, width }: ResultTableProps) {
  const gridRef = useRef<AgGridReact>(null);
  const colorMode = useStore((state) => state.functionalColorMode);
  const [gridApi, setGridApi] = useState<GridApi | null>(null);
  const [cellPopup, setCellPopup] = useState<{
    content: string;
    x: number;
    y: number;
  } | null>(null);

  const columnDefs: ColDef[] = useMemo(() => {
    return columns.map((col) => ({
      headerName: col.title,
      field: col.id,
      sortable: true,
      filter: true,
      resizable: true,
      width: col.width || undefined,
      flex: col.width ? 0 : 1, // Use flex if no specific width is set
      cellRenderer: (params: any) => {
        const value = params.value;
        if (value === null || value === undefined) {
          return "null";
        }
        if (typeof value === "object") {
          return JSON.stringify(value);
        }
        return String(value);
      },
    }));
  }, [columns]);

  const rowData = useMemo(() => {
    return results.map((row) => {
      const rowObj: any = {};
      columns.forEach((col, index) => {
        rowObj[col.id] = row[index];
      });
      return rowObj;
    });
  }, [results, columns]);

  const onColumnResized = (event: ColumnResizedEvent) => {
    if (setColumns && event.finished) {
      const newColumns = columns.map((col) => {
        const agCol = event.api.getColumn(col.id);
        if (agCol) {
          return {
            ...col,
            width: agCol.getActualWidth(),
          };
        }
        return col;
      });
      setColumns(newColumns);
    }
  };

  const defaultColDef = useMemo(
    () => ({
      sortable: true,
      filter: true,
      resizable: true,
      minWidth: 80,
    }),
    [],
  );

  const theme = useMemo(() => {
    let theme =
      colorMode === "dark"
        ? themeQuartz.withPart(colorSchemeDarkWarm)
        : themeQuartz.withPart(colorSchemeLightWarm);

    theme = theme.withParams({
      wrapperBorderRadius: 0,
    });

    return theme;
  }, [colorMode]);

  const onGridReady = (params: GridReadyEvent) => {
    setGridApi(params.api);
  };

  const onCellDoubleClicked = (event: CellDoubleClickedEvent) => {
    const value = event.value;
    const cellContent =
      value === null || value === undefined
        ? "null"
        : typeof value === "object"
          ? JSON.stringify(value, null, 2)
          : String(value);

    // Get the cell element position
    const cellElement = event.event?.target as HTMLElement;
    if (cellElement && gridApi) {
      const rect = cellElement.getBoundingClientRect();

      setCellPopup({
        content: cellContent,
        x: rect.left,
        y: rect.top - 10,
      });
    }
  };

  const onCellClicked = () => {
    // Dismiss popup when any cell is clicked
    if (cellPopup) {
      setCellPopup(null);
    }
  };

  useEffect(() => {
    // ag-grid enterprise/pro has some really cool clipboard features - and a lot of other things!
    // this makes it work in a bare-bones way for now, but tbh we should pay for the full version at some point.
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check if Cmd+C (Mac) or Ctrl+C (Windows/Linux) is pressed
      if ((event.metaKey || event.ctrlKey) && event.key === "c" && gridApi) {
        const focusedCell = gridApi.getFocusedCell();
        if (focusedCell) {
          const rowNode = gridApi.getDisplayedRowAtIndex(focusedCell.rowIndex);
          if (rowNode) {
            const cellValue = rowNode.data[focusedCell.column.getColId()];
            const textToCopy =
              cellValue === null || cellValue === undefined ? "" : String(cellValue);

            navigator.clipboard
              .writeText(textToCopy)
              .then(() => {
                console.log("Cell content copied to clipboard:", textToCopy);
              })
              .catch((err) => {
                console.error("Failed to copy to clipboard:", err);
              });
          }
        }
        event.preventDefault();
        event.stopPropagation();
      }
    };

    const handleClickOutside = (event: MouseEvent) => {
      if (cellPopup) {
        const target = event.target as HTMLElement;
        if (!target.closest(".cell-popup")) {
          setCellPopup(null);
        }
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("click", handleClickOutside);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("click", handleClickOutside);
    };
  }, [gridApi, cellPopup]);

  return (
    <>
      <div
        className="w-full h-full ag-grid"
        style={{ width, height: "100%" }}
        onClick={(e) => e.stopPropagation()}
        onMouseDown={(e) => e.stopPropagation()}
        onDragStart={(e) => {
          e.preventDefault();
          e.stopPropagation();
        }}
        onDragEnd={(e) => {
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <AgGridReact
          ref={gridRef}
          rowData={rowData}
          columnDefs={columnDefs}
          defaultColDef={defaultColDef}
          onGridReady={onGridReady}
          onColumnResized={onColumnResized}
          onCellDoubleClicked={onCellDoubleClicked}
          onCellClicked={onCellClicked}
          theme={theme}
          suppressColumnVirtualisation={true}
        />
      </div>

      {cellPopup &&
        createPortal(
          <div
            className="cell-popup fixed z-50 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded shadow-lg p-2 max-w-sm max-h-48 overflow-auto"
            style={{
              left: cellPopup.x,
              top: cellPopup.y,
              maxWidth: "300px",
              maxHeight: "200px",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <pre className="text-xs whitespace-pre-wrap font-mono text-gray-900 dark:text-gray-100">
              {cellPopup.content}
            </pre>
          </div>,
          document.body,
        )}
    </>
  );
}
