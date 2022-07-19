/** @type {import('ts-jest/dist/types').InitialOptionsTsJest} */
module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'jsdom',
  globals: {
    'ts-jest': {
      isolatedModules: true,
    },
  },
  transform: {
    '^.+\\.(ts|tsx|js|jsx)$': 'ts-jest',
  },
  testPathIgnorePatterns: ['node_modules/', '__tests__/mocks/', '__mocks__/'],
  transformIgnorePatterns: ['node_modules/(?!@tauri)'],
  cacheDirectory: '.jest/cache',
  moduleNameMapper: {
    '\\.(svg|png|jpg)$': '<rootDir>/__tests__/mocks/mockImages.js',
    '\\.(css|scss)$': '<rootDir>/__tests__/mocks/mockStyles.js',
  },
  setupFilesAfterEnv: ['<rootDir>/src/setupTests.ts'],
  collectCoverage: true,
  reporters: ['jest-junit'],
  coverageDirectory: 'temp/reports/tests',
  collectCoverageFrom: [
    'src/**/*.{ts,tsx}',
    '!src/reportWebVitals.ts',
    '!src/custom.d.ts',
    '!src/react-app-env.d.ts',
    '!src/index.tsx',
  ],
}
