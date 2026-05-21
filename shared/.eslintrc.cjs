// Legacy-format config so eslint picks it up even when shared/ is linted
// from web/'s flat config. We only need the restricted-imports rule.
module.exports = {
  root: false,
  rules: {
    'no-restricted-imports': [
      'error',
      {
        paths: [
          { name: 'react', message: 'shared/ must be platform-agnostic.' },
          { name: 'react-dom', message: 'shared/ must be platform-agnostic.' },
          { name: 'react-native', message: 'shared/ must be platform-agnostic.' },
        ],
        patterns: [
          { group: ['react-native*'], message: 'shared/ must be platform-agnostic.' },
          { group: ['@react-native*'], message: 'shared/ must be platform-agnostic.' },
        ],
      },
    ],
  },
}
