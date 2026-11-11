export { ApiClientError, handleResponseText } from './client.ts';
export { getDashboard } from './dashboard.ts';
export { getAuthMe, setupAdmin, login, logout, createEditor } from './auth.ts';
export { listUsers, createUser, updateUser, resetUserPassword, removeUser } from './auth.ts';
export { getRegistry, getPage, uploadTheme, previewBlock, saveBlock } from './builder.ts';
export { getThemes, installTheme, activateTheme } from './themes.ts';
export { getPlugins, installPlugin, runPlugin, disablePlugin, enablePlugin } from './plugins.ts';
export {
  listTaxonomyTerms, createTaxonomyTerm, updateTaxonomyTerm, deleteTaxonomyTerm,
} from './taxonomy.ts';
export type {
  TaxonomyKind, TaxonomyTerm, TaxonomyTermPage, TaxonomyTermInput, TaxonomyTermChanges,
} from './taxonomy.ts';
