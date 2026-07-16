import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import LanguageDetector from 'i18next-browser-languagedetector'

import enCommon from './locales/en/common.json'
import zhCNCommon from './locales/zh-CN/common.json'
import esCommon from './locales/es/common.json'
import frCommon from './locales/fr/common.json'
import deCommon from './locales/de/common.json'
import jaCommon from './locales/ja/common.json'

const isDev = import.meta.env.DEV && import.meta.env.MODE !== 'test'

// 初始化一次即可；React 组件通过 useTranslation() 获取 t/i18n
void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    debug: isDev,
    fallbackLng: 'en',
    supportedLngs: ['zh-CN', 'en', 'es', 'fr', 'de', 'ja'],
    // 仅按“当前选择的语言”加载/解析，避免把 zh-CN 降级成 zh 导致资源未命中
    load: 'currentOnly',
    defaultNS: 'common',
    ns: ['common'],
    interpolation: { escapeValue: false },
    detection: {
      order: ['localStorage', 'navigator'],
      lookupLocalStorage: 'lang',
      caches: ['localStorage'],
    },
    resources: {
      en: { common: enCommon },
      'zh-CN': { common: zhCNCommon },
      es: { common: esCommon },
      fr: { common: frCommon },
      de: { common: deCommon },
      ja: { common: jaCommon },
    },
  })

export { i18n }
