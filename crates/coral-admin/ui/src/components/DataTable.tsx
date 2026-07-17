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
    <table className="w-full border-separate border-spacing-0 text-sm">
      <thead>
        {table.getHeaderGroups().map((headerGroup) => (
          <tr key={headerGroup.id}>
            {headerGroup.headers.map((header) => (
              <th
                key={header.id}
                className="border-b border-white/8 pb-2 text-left text-[11px] font-medium tracking-wide text-gray-500 uppercase"
              >
                {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
              </th>
            ))}
          </tr>
        ))}
      </thead>
      <tbody>
        {data.length === 0 ? (
          <tr>
            <td colSpan={columns.length} className="py-6 text-center text-sm text-gray-500">
              {emptyMessage}
            </td>
          </tr>
        ) : (
          table.getRowModel().rows.map((row) => (
            <tr
              key={row.id}
              onClick={() => onRowClick?.(row.original)}
              className={`${onRowClick ? "cursor-pointer" : ""} hover:bg-white/4 ${rowClassName?.(row.original) ?? ""}`}
            >
              {row.getVisibleCells().map((cell) => (
                <td key={cell.id} className="border-b border-white/5 py-2.5 text-gray-200">
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
