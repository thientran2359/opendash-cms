import { useEffect, useState } from 'react';
import { getGeneralSettings } from '../api/settings.ts';

let cachedSiteTimezone: string | null = null;
let pendingRequest: Promise<string> | null = null;

function fetchSiteTimezone(): Promise<string> {
  if (cachedSiteTimezone) {
    return Promise.resolve(cachedSiteTimezone);
  }
  if (!pendingRequest) {
    pendingRequest = getGeneralSettings()
      .then((general) => {
        const tz = general.timezone?.trim() || 'UTC';
        cachedSiteTimezone = tz;
        return tz;
      })
      .catch(() => 'UTC')
      .finally(() => {
        pendingRequest = null;
      });
  }
  return pendingRequest;
}

export function useSiteTimezone(explicit?: string | null): string {
  const [resolved, setResolved] = useState<string>(
    explicit?.trim() || cachedSiteTimezone || 'UTC',
  );

  useEffect(() => {
    const trimmed = explicit?.trim();
    if (trimmed) {
      cachedSiteTimezone = trimmed;
      setResolved(trimmed);
      return;
    }
    let active = true;
    void fetchSiteTimezone().then((tz) => {
      if (active) setResolved(tz);
    });
    return () => {
      active = false;
    };
  }, [explicit]);

  return explicit?.trim() || resolved || 'UTC';
}
