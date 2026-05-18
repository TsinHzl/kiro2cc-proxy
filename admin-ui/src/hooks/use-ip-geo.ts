import { useState, useEffect } from 'react'

interface GeoInfo {
  country: string
  regionName: string
  city: string
}

const cache = new Map<string, GeoInfo | null>()

export function useIpGeo(ips: string[]): Map<string, GeoInfo | null> {
  const [result, setResult] = useState<Map<string, GeoInfo | null>>(new Map())

  useEffect(() => {
    const uniqueIps = [...new Set(ips)].filter(Boolean)
    if (uniqueIps.length === 0) return

    const uncached = uniqueIps.filter((ip) => !cache.has(ip))

    const applyCache = () => {
      const m = new Map<string, GeoInfo | null>()
      for (const ip of uniqueIps) {
        m.set(ip, cache.get(ip) ?? null)
      }
      setResult(m)
    }

    if (uncached.length === 0) {
      applyCache()
      return
    }

    const body = uncached.map((ip) => ({ query: ip, fields: 'query,country,regionName,city,status' }))
    fetch('http://ip-api.com/batch?lang=zh-CN', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
      .then((r) => r.json())
      .then((data: Array<{ query: string; status: string; country: string; regionName: string; city: string }>) => {
        for (const item of data) {
          cache.set(item.query, item.status === 'success' ? { country: item.country, regionName: item.regionName, city: item.city } : null)
        }
        applyCache()
      })
      .catch(() => {
        for (const ip of uncached) cache.set(ip, null)
        applyCache()
      })
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ips.join(',')])

  return result
}
