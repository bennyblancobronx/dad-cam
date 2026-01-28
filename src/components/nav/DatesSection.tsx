// Dad Cam - Left Nav Dates Section (Phase 7)
// Hierarchical Year > Month > Day tree navigation with keyboard support

import { useState, useEffect, useCallback, useMemo, useRef, KeyboardEvent } from 'react';
import type { DateGroup } from '../../types/events';
import { getClipsGroupedByDate } from '../../api/events';

// Month names for display
const MONTH_NAMES = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
];

// Short month names for compact display
const MONTH_NAMES_SHORT = [
  'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
  'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
];

interface DayNode {
  date: string;       // YYYY-MM-DD
  day: number;        // 1-31
  clipCount: number;
}

interface MonthNode {
  month: number;      // 1-12
  monthName: string;
  clipCount: number;
  days: DayNode[];
}

interface YearNode {
  year: number;
  clipCount: number;
  months: MonthNode[];
}

interface DatesSectionProps {
  onNavigateToDate?: (date: string) => void;
  /** Currently active date for highlighting */
  activeDate?: string | null;
  /** Increment to trigger a refresh (e.g., after importing clips) */
  refreshTrigger?: number;
}

export function DatesSection({ onNavigateToDate, activeDate, refreshTrigger }: DatesSectionProps) {
  const [dates, setDates] = useState<DateGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedYears, setExpandedYears] = useState<Set<number>>(new Set());
  const [expandedMonths, setExpandedMonths] = useState<Set<string>>(new Set());

  const loadDates = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const dateGroups = await getClipsGroupedByDate();
      setDates(dateGroups);

      // Auto-expand the most recent year
      if (dateGroups.length > 0) {
        const mostRecentDate = dateGroups[0].date;
        const year = parseInt(mostRecentDate.split('-')[0], 10);
        setExpandedYears(new Set([year]));
      }
    } catch (err) {
      console.error('Failed to load dates:', err);
      setError(err instanceof Error ? err.message : 'Failed to load dates');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadDates();
  }, [loadDates, refreshTrigger]);

  // Transform flat date list into hierarchical tree structure
  const dateTree = useMemo((): YearNode[] => {
    const yearMap = new Map<number, Map<number, DayNode[]>>();

    for (const group of dates) {
      const [yearStr, monthStr, dayStr] = group.date.split('-');
      const year = parseInt(yearStr, 10);
      const month = parseInt(monthStr, 10);
      const day = parseInt(dayStr, 10);

      if (!yearMap.has(year)) {
        yearMap.set(year, new Map());
      }
      const monthMap = yearMap.get(year)!;

      if (!monthMap.has(month)) {
        monthMap.set(month, []);
      }
      monthMap.get(month)!.push({
        date: group.date,
        day,
        clipCount: group.clipCount,
      });
    }

    // Convert to array structure and calculate totals
    const years: YearNode[] = [];

    for (const [year, monthMap] of yearMap) {
      const months: MonthNode[] = [];
      let yearClipCount = 0;

      for (const [month, days] of monthMap) {
        const monthClipCount = days.reduce((sum, d) => sum + d.clipCount, 0);
        yearClipCount += monthClipCount;

        // Sort days descending
        days.sort((a, b) => b.day - a.day);

        months.push({
          month,
          monthName: MONTH_NAMES[month - 1],
          clipCount: monthClipCount,
          days,
        });
      }

      // Sort months descending
      months.sort((a, b) => b.month - a.month);

      years.push({
        year,
        clipCount: yearClipCount,
        months,
      });
    }

    // Sort years descending
    years.sort((a, b) => b.year - a.year);

    return years;
  }, [dates]);

  const toggleYear = (year: number) => {
    setExpandedYears(prev => {
      const next = new Set(prev);
      if (next.has(year)) {
        next.delete(year);
      } else {
        next.add(year);
      }
      return next;
    });
  };

  const toggleMonth = (yearMonth: string) => {
    setExpandedMonths(prev => {
      const next = new Set(prev);
      if (next.has(yearMonth)) {
        next.delete(yearMonth);
      } else {
        next.add(yearMonth);
      }
      return next;
    });
  };

  // Keyboard navigation handler for tree items
  const treeRef = useRef<HTMLDivElement>(null);

  const handleKeyDown = useCallback((
    e: KeyboardEvent<HTMLButtonElement>,
    type: 'year' | 'month' | 'day',
    data: { year?: number; yearMonth?: string; date?: string }
  ) => {
    const key = e.key;

    // Handle expand/collapse and selection
    if (key === 'Enter' || key === ' ') {
      e.preventDefault();
      if (type === 'year' && data.year !== undefined) {
        toggleYear(data.year);
      } else if (type === 'month' && data.yearMonth) {
        toggleMonth(data.yearMonth);
      } else if (type === 'day' && data.date && onNavigateToDate) {
        onNavigateToDate(data.date);
      }
      return;
    }

    // Handle arrow key navigation
    if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(key)) {
      return;
    }

    e.preventDefault();
    const tree = treeRef.current;
    if (!tree) return;

    // Get all visible buttons in the tree
    const buttons = Array.from(tree.querySelectorAll('button:not([disabled])')) as HTMLButtonElement[];
    const currentIndex = buttons.indexOf(e.currentTarget);
    if (currentIndex === -1) return;

    let nextIndex = currentIndex;

    if (key === 'ArrowDown') {
      nextIndex = Math.min(currentIndex + 1, buttons.length - 1);
    } else if (key === 'ArrowUp') {
      nextIndex = Math.max(currentIndex - 1, 0);
    } else if (key === 'ArrowRight') {
      // Expand if collapsed
      if (type === 'year' && data.year !== undefined && !expandedYears.has(data.year)) {
        toggleYear(data.year);
      } else if (type === 'month' && data.yearMonth && !expandedMonths.has(data.yearMonth)) {
        toggleMonth(data.yearMonth);
      }
      return;
    } else if (key === 'ArrowLeft') {
      // Collapse if expanded, or move to parent
      if (type === 'year' && data.year !== undefined && expandedYears.has(data.year)) {
        toggleYear(data.year);
      } else if (type === 'month' && data.yearMonth && expandedMonths.has(data.yearMonth)) {
        toggleMonth(data.yearMonth);
      } else if (type === 'month' || type === 'day') {
        // Move focus to parent using data-tree-level attribute
        nextIndex = Math.max(currentIndex - 1, 0);
        while (nextIndex > 0) {
          const level = parseInt(buttons[nextIndex].dataset.treeLevel || '0', 10);
          if (type === 'day' && level <= 1) break; // Day stops at month (1) or year (0)
          if (type === 'month' && level === 0) break; // Month stops at year (0)
          nextIndex--;
        }
      }
    }

    if (nextIndex !== currentIndex) {
      buttons[nextIndex].focus();
    }
  }, [expandedYears, expandedMonths, toggleYear, toggleMonth, onNavigateToDate]);

  return (
    <div className="nav-section">
      <div className="nav-section-header">
        <svg className="nav-section-icon" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
          <rect x="3" y="4" width="14" height="14" rx="2" />
          <path d="M3 8h14M7 4V2M13 4V2" />
        </svg>
        <h3 className="nav-section-title">Dates</h3>
      </div>

      {loading ? (
        <div className="nav-loading">Loading...</div>
      ) : error ? (
        <div className="nav-error">{error}</div>
      ) : dateTree.length === 0 ? (
        <div className="nav-empty">
          <span className="nav-empty-text">No recordings yet</span>
        </div>
      ) : (
        <div className="nav-dates-tree" ref={treeRef} role="tree">
          {dateTree.map((yearNode) => {
            const isYearExpanded = expandedYears.has(yearNode.year);

            return (
              <div key={yearNode.year} className="nav-tree-year" role="treeitem" aria-expanded={isYearExpanded}>
                <button
                  className="nav-item nav-tree-toggle"
                  data-tree-level="0"
                  onClick={() => toggleYear(yearNode.year)}
                  onKeyDown={(e) => handleKeyDown(e, 'year', { year: yearNode.year })}
                  aria-label={`${yearNode.year}, ${yearNode.clipCount} clips`}
                >
                  <svg
                    className={`nav-tree-chevron ${isYearExpanded ? 'is-expanded' : ''}`}
                    width="12"
                    height="12"
                    viewBox="0 0 12 12"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                  >
                    <path d="M4 2l4 4-4 4" />
                  </svg>
                  <span className="nav-tree-label">{yearNode.year}</span>
                  <span className="nav-tree-count">{yearNode.clipCount}</span>
                </button>

                {isYearExpanded && (
                  <div className="nav-tree-children">
                    {yearNode.months.map((monthNode) => {
                      const yearMonthKey = `${yearNode.year}-${monthNode.month}`;
                      const isMonthExpanded = expandedMonths.has(yearMonthKey);

                      return (
                        <div key={yearMonthKey} className="nav-tree-month" role="treeitem" aria-expanded={isMonthExpanded}>
                          <button
                            className="nav-item nav-tree-toggle nav-tree-indent-1"
                            data-tree-level="1"
                            onClick={() => toggleMonth(yearMonthKey)}
                            onKeyDown={(e) => handleKeyDown(e, 'month', { yearMonth: yearMonthKey })}
                            aria-label={`${monthNode.monthName}, ${monthNode.clipCount} clips`}
                          >
                            <svg
                              className={`nav-tree-chevron ${isMonthExpanded ? 'is-expanded' : ''}`}
                              width="12"
                              height="12"
                              viewBox="0 0 12 12"
                              fill="none"
                              stroke="currentColor"
                              strokeWidth="2"
                            >
                              <path d="M4 2l4 4-4 4" />
                            </svg>
                            <span className="nav-tree-label">{monthNode.monthName}</span>
                            <span className="nav-tree-count">{monthNode.clipCount}</span>
                          </button>

                          {isMonthExpanded && (
                            <div className="nav-tree-children">
                              {monthNode.days.map((dayNode) => (
                                <button
                                  key={dayNode.date}
                                  role="treeitem"
                                  className={`nav-item nav-tree-day nav-tree-indent-2${activeDate === dayNode.date ? ' is-active' : ''}`}
                                  data-tree-level="2"
                                  onClick={() => onNavigateToDate?.(dayNode.date)}
                                  onKeyDown={(e) => handleKeyDown(e, 'day', { date: dayNode.date })}
                                  aria-label={`${MONTH_NAMES_SHORT[monthNode.month - 1]} ${dayNode.day}, ${dayNode.clipCount} clips`}
                                  aria-selected={activeDate === dayNode.date}
                                >
                                  <span className="nav-tree-label">
                                    {MONTH_NAMES_SHORT[monthNode.month - 1]} {dayNode.day}
                                  </span>
                                  <span className="nav-tree-count">{dayNode.clipCount}</span>
                                </button>
                              ))}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
