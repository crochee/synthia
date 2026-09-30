import type { ReactNode } from 'react';

/**
 * Single cell in a `<MetadataTable>`. `value` accepts a React
 * node so callers can render pills, links, or short inline
 * markdown — the table itself never decides formatting.
 */
export interface MetadataRow {
  label: string;
  value: ReactNode;
}

/**
 * Metadata table — the canonical "label / value" surface used by
 * Agent and Skill detail/list pages.
 *
 * Renders a two-column `<table>` with `<th>` labels in the
 * design tokens' primary accent and `<td>` values in body text.
 * Rows with `undefined`/`null`/empty-string values are skipped
 * so callers can build a single array of rows without conditional
 * filters.
 *
 * Wrapped in `React.memo` because the typical caller
 * (`SkillsPage` / `AgentsPage`) builds the `rows` array inline
 * at every render — without memo, every keystroke in the search
 * box would re-render every table for every list item, even
 * though the table content is identical between renders. With
 * memo, the table only re-renders when `rows` identity changes
 * (i.e. a new skill/agent arrived in the cursor-paginated list).
 */
export interface MetadataTableProps {
  rows: MetadataRow[];
}

function isEmpty(value: ReactNode): boolean {
  if (value === null || value === undefined) return true;
  if (typeof value === 'string') return value.trim().length === 0;
  return false;
}

export function MetadataTable({ rows }: MetadataTableProps): ReactNode {
  const visible = rows.filter((r) => !isEmpty(r.value));
  if (visible.length === 0) return null;
  return (
    <table className="nt-meta-table">
      <tbody>
        {visible.map((r) => (
          <tr key={r.label}>
            <th scope="row">{r.label}</th>
            <td>{r.value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
