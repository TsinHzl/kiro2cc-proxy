// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect } from 'react'
import { getGeoBatch } from '@/api/credentials'

export interface GeoInfo {
  country: string
  regionName: string
  city: string
  displayIp?: string
}

const geoCache = new Map<string, GeoInfo | null>()

async function fetchGeoForIps(ips: string[]): Promise<void> {
  if (ips.length === 0) return
  try {
    const data = await getGeoBatch(ips)
    for (const ip of ips) geoCache.set(ip, data[ip] ?? null)
  } catch {
    for (const ip of ips) geoCache.set(ip, null)
  }
}

export function useIpGeo(ips: string[]): Map<string, GeoInfo | null> {
  const [result, setResult] = useState<Map<string, GeoInfo | null>>(new Map())

  useEffect(() => {
    const uniqueIps = [...new Set(ips)].filter(Boolean)
    if (uniqueIps.length === 0) return

    const run = async () => {
      const uncached = uniqueIps.filter((ip) => !geoCache.has(ip))
      await fetchGeoForIps(uncached)

      const m = new Map<string, GeoInfo | null>()
      for (const ip of uniqueIps) m.set(ip, geoCache.get(ip) ?? null)
      setResult(m)
    }

    run()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ips.join(',')])

  return result
}
