import { flexRender, getCoreRowModel, useReactTable, type ColumnDef } from "@tanstack/react-table";

type DataTableProps<T> = {
  columns: ColumnDef<T, unknown>[];
  data: T[];
  onRowClick?: (row: T) => void;
  rowClassName?: (row: T) => string;
  emptyMessage?: string;
};

export function DataTable<T>({ columns, data, onRowClick, rowClassName, emptyMessage = "No data" }: DataTableProps<T>) {
  const table = useReactTable({
    data,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  return (
    <table className="w-full text-sm">
      <thead>
        {table.getHeaderGroups().map((headerGroup) => (
          <tr key={headerGroup.id} className="text-left text-xs text-gray-500">
            {headerGroup.headers.map((header) => (
              <th key={header.id} className="pb-1 font-normal">
                {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
              </th>
            ))}
          </tr>
        ))}
      </thead>
      <tbody>
        {data.length === 0 ? (
          <tr>
            <td colSpan={columns.length} className="py-4 text-center text-gray-500">
              {emptyMessage}
            </td>
          </tr>
        ) : (
          table.getRowModel().rows.map((row) => (
            <tr
              key={row.id}
              onClick={() => onRowClick?.(row.original)}
              className={`border-t border-white/5 ${onRowClick ? "cursor-pointer hover:bg-white/5" : ""} ${rowClassName?.(row.original) ?? ""}`}
            >
              {row.getVisibleCells().map((cell) => (
                <td key={cell.id} className="py-1.5">
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </td>
              ))}
            </tr>
          ))
        )}
      </tbody>
    </table>
  );
}
