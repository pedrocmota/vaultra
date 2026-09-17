module.exports = {
  root: true,
  env: {
    es2021: true,
    browser: true
  },
  parser: '@typescript-eslint/parser',
  parserOptions: {
    ecmaVersion: 12,
    sourceType: 'module',
    ecmaFeatures: {
      jsx: true
    }
  },
  plugins: ['@typescript-eslint', 'eslint-plugin-import-helpers'],
  extends: ['plugin:react/recommended'],
  ignorePatterns: ['node_modules/', 'dist/', 'src-tauri/', 'scripts/', '**/*.html'],
  rules: {
    curly: ['warn', 'all'],
    'brace-style': ['warn', '1tbs', {allowSingleLine: false}],
    'space-before-blocks': ['warn', 'always'],
    quotes: ['warn', 'single'],
    semi: ['warn', 'never'],
    'eol-last': ['warn', 'never'],
    indent: ['warn', 2, {SwitchCase: 1}],
    'comma-dangle': ['warn', 'never'],
    'quote-props': ['warn', 'as-needed'],
    'space-in-parens': ['warn', 'never'],
    'no-multi-spaces': ['warn', {exceptions: {Property: false}}],
    'no-empty-function': 'off',
    'no-empty': 'off',
    'no-empty-pattern': 'off',
    'no-var': 'off',
    'no-unused-vars': 'off',
    'prefer-const': 'off',
    'no-constant-condition': 'off',
    '@typescript-eslint/no-this-alias': 'off',
    '@typescript-eslint/no-var-requires': 'off',
    '@typescript-eslint/ban-types': 'off',
    '@typescript-eslint/no-explicit-any': 'off',
    '@typescript-eslint/no-unused-vars': 'off',
    '@typescript-eslint/no-empty-interface': 'off',
    '@typescript-eslint/explicit-module-boundary-types': 'off',
    '@typescript-eslint/no-empty-function': 'off',
    '@typescript-eslint/no-non-null-assertion': 'off',
    '@typescript-eslint/ban-ts-comment': 'off',
    '@typescript-eslint/no-namespace': 'off',
    '@typescript-eslint/naming-convention': [
      'warn',
      {
        selector: 'interface',
        format: ['PascalCase']
      }
    ],
    '@typescript-eslint/member-delimiter-style': [
      'warn',
      {
        multiline: {
          delimiter: 'comma',
          requireLast: false
        },
        singleline: {
          delimiter: 'comma',
          requireLast: false
        }
      }
    ],
    'react/react-in-jsx-scope': 'off',
    'react/prop-types': 'off',
    'react/no-children-prop': 'off',
    'react/display-name': 'off',
    'react/jsx-curly-brace-presence': [
      'warn',
      {
        children: 'ignore',
        props: 'never'
      }
    ],
    'jsx-quotes': ['warn', 'prefer-double'],
    'import-helpers/order-imports': [
      'warn',
      {
        newlinesBetween: 'never',
        groups: [
          '/^react/',
          'module',
          '/^@\//',
          ['parent', 'sibling', 'index']
        ],
        alphabetize: {
          order: 'asc',
          ignoreCase: true
        }
      }
    ],
    'padding-line-between-statements': [
      'warn',
      {blankLine: 'always', prev: '*', next: 'return'},
      {blankLine: 'always', prev: '*', next: 'if'},
      {blankLine: 'always', prev: 'if', next: '*'}
    ],
    'padding-between-multiline-const': 'warn',
    'max-if-condition-length': ['warn', {maxLength: 120}],
    'no-padded-objects': 'warn',
    'max-call-length': ['warn', {maxLength: 100}],
    'max-params-length': ['warn', {maxLength: 100}],
    'max-array-length': ['warn', {maxLength: 100}],
    'prefer-alias-imports': ['warn', {alias: '@', baseUrl: 'src'}],
    'constructor-params-indent': ['warn', {indent: 2}],
    'try-catch-padding': 'warn',
    'no-padded-blocks': 'warn'
  },
  overrides: [
    {
      files: ['**/*.d.ts'],
      rules: {
        '@typescript-eslint/no-unused-vars': 'off',
        'no-unused-vars': 'off'
      }
    }
  ],
  settings: {
    react: {
      version: 'detect'
    }
  }
}
