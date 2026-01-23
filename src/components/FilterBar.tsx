// Dad Cam - Phase 3 Filter Bar Component
import { useState, useCallback, useEffect } from 'react';
import type { FilterType, SortField, SortOrder } from '../types/clips';

interface FilterBarProps {
  filter: FilterType;
  onFilterChange: (filter: FilterType) => void;
  search: string;
  onSearchChange: (search: string) => void;
  sortBy: SortField;
  sortOrder: SortOrder;
  onSortChange: (sortBy: SortField, sortOrder: SortOrder) => void;
  dateFrom?: string;
  dateTo?: string;
  onDateRangeChange: (dateFrom?: string, dateTo?: string) => void;
  totalClips: number;
  displayedClips: number;
}

export function FilterBar({
  filter,
  onFilterChange,
  search,
  onSearchChange,
  sortBy,
  sortOrder,
  onSortChange,
  dateFrom,
  dateTo,
  onDateRangeChange,
  totalClips,
  displayedClips,
}: FilterBarProps) {
  const [searchInput, setSearchInput] = useState(search);
  const [localDateFrom, setLocalDateFrom] = useState(dateFrom || '');
  const [localDateTo, setLocalDateTo] = useState(dateTo || '');

  // Sync local state with props
  useEffect(() => {
    setSearchInput(search);
  }, [search]);

  useEffect(() => {
    setLocalDateFrom(dateFrom || '');
  }, [dateFrom]);

  useEffect(() => {
    setLocalDateTo(dateTo || '');
  }, [dateTo]);

  // Debounced search
  useEffect(() => {
    const timeoutId = setTimeout(() => {
      if (searchInput !== search) {
        onSearchChange(searchInput);
      }
    }, 300);
    return () => clearTimeout(timeoutId);
  }, [searchInput, search, onSearchChange]);

  const handleSearchInput = useCallback((value: string) => {
    setSearchInput(value);
  }, []);

  // Handle date range changes
  const handleDateFromChange = useCallback((value: string) => {
    setLocalDateFrom(value);
    onDateRangeChange(value || undefined, localDateTo || undefined);
  }, [localDateTo, onDateRangeChange]);

  const handleDateToChange = useCallback((value: string) => {
    setLocalDateTo(value);
    onDateRangeChange(localDateFrom || undefined, value || undefined);
  }, [localDateFrom, onDateRangeChange]);

  const handleClearDates = useCallback(() => {
    setLocalDateFrom('');
    setLocalDateTo('');
    onDateRangeChange(undefined, undefined);
  }, [onDateRangeChange]);

  const filterButtons: { value: FilterType; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'favorites', label: 'Favorites' },
    { value: 'bad', label: 'Bad' },
    { value: 'unreviewed', label: 'Unreviewed' },
  ];

  const sortOptions: { value: SortField; label: string }[] = [
    { value: 'recorded_at', label: 'Date Recorded' },
    { value: 'title', label: 'Title' },
    { value: 'created_at', label: 'Date Added' },
  ];

  const hasDateFilter = localDateFrom || localDateTo;

  return (
    <div
      className="filter-bar"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '16px',
        padding: '12px 16px',
        backgroundColor: '#1a1a1a',
        borderBottom: '1px solid #333',
        flexWrap: 'wrap',
      }}
    >
      {/* Filter buttons */}
      <div style={{ display: 'flex', gap: '4px' }}>
        {filterButtons.map(({ value, label }) => (
          <button
            key={value}
            onClick={() => onFilterChange(value)}
            style={{
              padding: '6px 12px',
              border: 'none',
              borderRadius: '4px',
              cursor: 'pointer',
              backgroundColor: filter === value ? '#4a9eff' : '#333',
              color: filter === value ? 'white' : '#ccc',
              fontSize: '13px',
            }}
          >
            {label}
          </button>
        ))}
      </div>

      {/* Search input */}
      <div style={{ flex: 1, minWidth: '150px', maxWidth: '300px' }}>
        <input
          type="text"
          placeholder="Search clips..."
          value={searchInput}
          onChange={(e) => handleSearchInput(e.target.value)}
          style={{
            width: '100%',
            padding: '8px 12px',
            border: '1px solid #444',
            borderRadius: '4px',
            backgroundColor: '#2a2a2a',
            color: 'white',
            fontSize: '14px',
          }}
        />
      </div>

      {/* Date range filter */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        <span style={{ color: '#888', fontSize: '13px' }}>Date:</span>
        <input
          type="date"
          value={localDateFrom}
          onChange={(e) => handleDateFromChange(e.target.value)}
          style={{
            padding: '6px 8px',
            border: '1px solid #444',
            borderRadius: '4px',
            backgroundColor: '#2a2a2a',
            color: 'white',
            fontSize: '13px',
          }}
          title="From date"
        />
        <span style={{ color: '#666' }}>-</span>
        <input
          type="date"
          value={localDateTo}
          onChange={(e) => handleDateToChange(e.target.value)}
          style={{
            padding: '6px 8px',
            border: '1px solid #444',
            borderRadius: '4px',
            backgroundColor: '#2a2a2a',
            color: 'white',
            fontSize: '13px',
          }}
          title="To date"
        />
        {hasDateFilter && (
          <button
            onClick={handleClearDates}
            style={{
              padding: '4px 8px',
              border: '1px solid #444',
              borderRadius: '4px',
              backgroundColor: '#2a2a2a',
              color: '#888',
              cursor: 'pointer',
              fontSize: '12px',
            }}
            title="Clear date filter"
          >
            Clear
          </button>
        )}
      </div>

      {/* Sort controls */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        <span style={{ color: '#888', fontSize: '13px' }}>Sort:</span>
        <select
          value={sortBy}
          onChange={(e) => onSortChange(e.target.value as SortField, sortOrder)}
          style={{
            padding: '6px 8px',
            border: '1px solid #444',
            borderRadius: '4px',
            backgroundColor: '#2a2a2a',
            color: 'white',
            fontSize: '13px',
          }}
        >
          {sortOptions.map(({ value, label }) => (
            <option key={value} value={value}>{label}</option>
          ))}
        </select>
        <button
          onClick={() => onSortChange(sortBy, sortOrder === 'asc' ? 'desc' : 'asc')}
          style={{
            padding: '6px 8px',
            border: '1px solid #444',
            borderRadius: '4px',
            backgroundColor: '#2a2a2a',
            color: 'white',
            cursor: 'pointer',
            fontSize: '14px',
          }}
          title={sortOrder === 'asc' ? 'Ascending' : 'Descending'}
        >
          {sortOrder === 'asc' ? '\u2191' : '\u2193'}
        </button>
      </div>

      {/* Clip count */}
      <div style={{ color: '#888', fontSize: '13px' }}>
        {displayedClips} of {totalClips} clips
      </div>
    </div>
  );
}
