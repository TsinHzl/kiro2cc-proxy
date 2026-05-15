const STORAGE_KEY = 'userApiKey'

export const storage = {
  getApiKey: () => localStorage.getItem(STORAGE_KEY),
  setApiKey: (key: string) => localStorage.setItem(STORAGE_KEY, key),
  removeApiKey: () => localStorage.removeItem(STORAGE_KEY),
}
