---
name: chronos
description: Authoritative date and time context from system clock. Use before any date calculations, weekly/monthly reporting, relative date references, quarter boundaries, or epoch timestamps to eliminate AI calculation errors.
---

# Chronos Skill

Eliminates AI date calculation errors by providing authoritative date and time context from the system clock.

## Problem

LLMs frequently make errors when calculating:
- Day of week from a date
- Week boundaries (Monday-Friday)
- Month/quarter boundaries
- Relative dates ("last Tuesday", "3 days ago")
- Year boundaries (especially Dec/Jan)
- ISO week numbers
- Business day counts

## Solution

Use `chronos.sh` to get authoritative date information. Never calculate dates manually.

## Usage

```bash
# Default: week context (backward-compatible)
chronos.sh

# Specific subcommands
chronos.sh week
chronos.sh month
chronos.sh quarter
chronos.sh relative "3 days ago"
chronos.sh iso
chronos.sh epoch
chronos.sh tz
chronos.sh range 2026-01-01 2026-02-01
chronos.sh all
```

## Subcommands

### `week` (default)

Week boundaries for reporting. This is the default when no subcommand is given.

```
DATE_CONTEXT:
  TODAY: 2026-02-17 (Tuesday)
  CURR_WEEK_START: 2026-02-16 (Monday)
  CURR_WEEK_END: 2026-02-20 (Friday)
  CURR_WEEK_RANGE: Feb 16 - Feb 20, 2026
  PREV_WEEK_START: 2026-02-09 (Monday)
  PREV_WEEK_END: 2026-02-13 (Friday)
  PREV_WEEK_RANGE: Feb 9 - Feb 13, 2026
```

### `month`

Current and previous month boundaries.

```
MONTH_CONTEXT:
  TODAY: 2026-02-17 (Tuesday)
  CURR_MONTH_START: 2026-02-01
  CURR_MONTH_END: 2026-02-28
  CURR_MONTH_RANGE: Feb 1 - Feb 28, 2026
  PREV_MONTH_START: 2026-01-01
  PREV_MONTH_END: 2026-01-31
  PREV_MONTH_RANGE: Jan 1, 2026 - Jan 31, 2026
```

### `quarter`

Calendar quarter, fiscal year (Oct-Sep), and fiscal quarter.

```
QUARTER_CONTEXT:
  TODAY: 2026-02-17 (Tuesday)
  CALENDAR_QUARTER: Q1 2026
  QUARTER_START: 2026-01-01
  QUARTER_END: 2026-03-31
  FISCAL_YEAR: FY2026 (Oct-Sep)
  FISCAL_QUARTER: FQ2
  FY_START: 2025-10-01
  FY_END: 2026-09-30
```

### `relative "expression"`

Resolve natural language date expressions via platform date.

```bash
chronos.sh relative "3 days ago"
chronos.sh relative "next Monday"
chronos.sh relative "yesterday"
```

```
RELATIVE_DATE:
  EXPRESSION: 3 days ago
  RESOLVED: 2026-02-14 (Friday)
  TODAY: 2026-02-17 (Tuesday)
```

Supported expressions (BSD): "N days ago", "N days from now", "yesterday", "tomorrow", "next/last Monday/Friday", "N weeks ago", "N months ago". GNU date supports all `date -d` expressions.

### `iso`

ISO 8601 week number, ISO year, and day-of-year.

```
ISO_CONTEXT:
  TODAY: 2026-02-17 (Tuesday)
  ISO_WEEK: W08
  ISO_YEAR: 2026
  ISO_WEEKDATE: 2026-W08-2
  DAY_OF_YEAR: 048
```

### `epoch`

Unix timestamp in seconds and milliseconds.

```
EPOCH_CONTEXT:
  TODAY: 2026-02-17 (Tuesday)
  UNIX_SECONDS: 1739836800
  UNIX_MILLIS: 1739836800000
```

### `tz`

Current timezone abbreviation and UTC offset.

```
TIMEZONE_CONTEXT:
  TODAY: 2026-02-17 (Tuesday)
  TIMEZONE: EST
  UTC_OFFSET: -0500
```

### `range DATE1 DATE2`

Calendar days and business days between two dates.

```bash
chronos.sh range 2026-01-01 2026-02-01
```

```
RANGE_CONTEXT:
  FROM: 2026-01-01
  TO: 2026-02-01
  CALENDAR_DAYS: 31
  BUSINESS_DAYS: 22
```

### `all`

Outputs all subcommands (week + month + quarter + iso + epoch + tz) combined.

## When to Use

Invoke this skill before any operation involving:
- Weekly reporting or logging
- Monthly summaries or boundaries
- Quarter or fiscal year references
- Scheduling references ("this week", "last month", "Q2")
- Date-stamped entries
- Relative date calculations ("3 days ago", "next Friday")
- ISO week numbers or day-of-year
- Unix timestamps or epoch conversions
- Timezone-aware operations
- Business day counts between dates

## Platform Support

Cross-platform: macOS (BSD date) and Linux (GNU date). The `relative` subcommand supports more expressions on GNU date.

## Integration Examples

### Weekly Reports
```bash
DATE_INFO=$(chronos.sh)
WEEK_RANGE=$(echo "$DATE_INFO" | grep "CURR_WEEK_RANGE:" | cut -d: -f2- | xargs)
echo "# Weekly Report: $WEEK_RANGE"
```

### Monthly Reports
```bash
MONTH_INFO=$(chronos.sh month)
MONTH_RANGE=$(echo "$MONTH_INFO" | grep "CURR_MONTH_RANGE:" | cut -d: -f2- | xargs)
echo "# Monthly Summary: $MONTH_RANGE"
```

## Key Principle

**External source of truth > internal calculation.**

The system clock is authoritative. Use it.
