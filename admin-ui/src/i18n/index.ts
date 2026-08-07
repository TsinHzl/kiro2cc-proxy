// Copyright (c) 2026 Harllan He. Licensed under MIT.
import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import zh from './locales/zh.json'
import en from './locales/en.json'

export const LANG_STORAGE_KEY = 'admin-lang'

const storedLang = localStorage.getItem(LANG_STORAGE_KEY)
const initialLang = storedLang === 'en' ? 'en' : 'zh'

i18n.use(initReactI18next).init({
  resources: {
    zh: { translation: zh },
    en: { translation: en },
  },
  lng: initialLang,
  fallbackLng: 'zh',
  interpolation: { escapeValue: false },
})

export default i18n
