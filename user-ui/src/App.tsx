import { useState, useEffect } from 'react'
import { storage } from '@/lib/storage'
import { login } from '@/api/user'
import { LoginPage } from '@/components/login-page'
import { Dashboard } from '@/components/dashboard'
import { Toaster } from '@/components/ui/sonner'
import type { LoginResponse } from '@/types/api'

function App() {
  const [isLoggedIn, setIsLoggedIn] = useState(false)
  const [checking, setChecking] = useState(true)

  useEffect(() => {
    // 检查已保存的 API Key 是否仍然有效
    const savedKey = storage.getApiKey()
    if (savedKey) {
      login(savedKey)
        .then(() => setIsLoggedIn(true))
        .catch(() => storage.removeApiKey())
        .finally(() => setChecking(false))
    } else {
      setChecking(false)
    }
  }, [])

  const handleLogin = (_data: LoginResponse) => {
    setIsLoggedIn(true)
  }

  const handleLogout = () => {
    setIsLoggedIn(false)
  }

  if (checking) return null

  return (
    <>
      {isLoggedIn ? (
        <Dashboard onLogout={handleLogout} />
      ) : (
        <LoginPage onLogin={handleLogin} />
      )}
      <Toaster position="top-right" />
    </>
  )
}

export default App
