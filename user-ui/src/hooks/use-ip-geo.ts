import { useState, useEffect } from 'react'

export interface GeoInfo {
  country: string
  regionName: string
  city: string
  displayIp?: string
}

const geoCache = new Map<string, GeoInfo | null>()
let publicIpCache: string | null | undefined = undefined // undefined = not fetched yet

function isLocalhost(ip: string) {
  return ip === '127.0.0.1' || ip === '::1' || ip.startsWith('::ffff:127.')
}

async function getPublicIp(): Promise<string | null> {
  if (publicIpCache !== undefined) return publicIpCache
  try {
    const res = await fetch('https://api.ipify.org?format=json')
    const data = await res.json()
    publicIpCache = typeof data.ip === 'string' ? data.ip : null
  } catch {
    publicIpCache = null
  }
  return publicIpCache ?? null
}

async function fetchGeoForIps(ips: string[]): Promise<void> {
  if (ips.length === 0) return
  const body = ips.map((ip) => ({ query: ip, fields: 'query,country,regionName,city,status' }))
  try {
    const res = await fetch('http://ip-api.com/batch?lang=zh-CN', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    const data: Array<{ query: string; status: string; country: string; regionName: string; city: string }> = await res.json()
    for (const item of data) {
      geoCache.set(item.query, item.status === 'success' ? { country: item.country, regionName: item.regionName, city: item.city } : null)
    }
  } catch {
    for (const ip of ips) geoCache.set(ip, null)
  }
}

export function useIpGeo(ips: string[]): Map<string, GeoInfo | null> {
  const [result, setResult] = useState<Map<string, GeoInfo | null>>(new Map())

  useEffect(() => {
    const uniqueIps = [...new Set(ips)].filter(Boolean)
    if (uniqueIps.length === 0) return

    const localhostIps = uniqueIps.filter(isLocalhost)
    const regularIps = uniqueIps.filter((ip) => !isLocalhost(ip))

    const applyCache = (publicIp: string | null) => {
      const m = new Map<string, GeoInfo | null>()
      for (const ip of uniqueIps) {
        if (isLocalhost(ip)) {
          const geo = publicIp ? (geoCache.get(publicIp) ?? null) : null
          m.set(ip, geo ? { ...geo, displayIp: publicIp ?? undefined } : publicIp ? { country: '', regionName: '', city: '', displayIp: publicIp } : null)
        } else {
          m.set(ip, geoCache.get(ip) ?? null)
        }
      }
      setResult(m)
    }

    const run = async () => {
      // 解析普通 IP 的归属地
      const uncachedRegular = regularIps.filter((ip) => !geoCache.has(ip))
      await fetchGeoForIps(uncachedRegular)

      // 处理 localhost：获取公网 IP 并解析其归属地
      let publicIp: string | null = null
      if (localhostIps.length > 0) {
        publicIp = await getPublicIp()
        if (publicIp && !geoCache.has(publicIp)) {
          await fetchGeoForIps([publicIp])
        }
      }

      applyCache(publicIp)
    }

    run()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ips.join(',')])

  return result
}
