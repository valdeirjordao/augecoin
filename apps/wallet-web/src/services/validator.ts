// AUGECOIN Wallet Web — Validador service (plans, orders, license, overview).

import { apiFetch } from './api';
import type {
  Plan,
  ValidatorOrder,
  ValidatorOverview,
  LicenseView,
} from '../types';

export function listPlans(): Promise<{ plans: Plan[] }> {
  return apiFetch('/validator/plans');
}

export function listOrders(): Promise<{ orders: ValidatorOrder[] }> {
  return apiFetch('/validator/orders');
}

export function createOrder(args: { plan: string; method: string }): Promise<{ order: ValidatorOrder }> {
  return apiFetch('/validator/orders', { method: 'POST', body: args, csrf: true });
}

export function issueLicense(orderId: string): Promise<{ license: LicenseView; license_key: string }> {
  return apiFetch(`/validator/orders/${orderId}/issue`, { method: 'POST', body: {}, csrf: true });
}

export function getOverview(): Promise<ValidatorOverview> {
  return apiFetch('/validator/overview');
}

export interface Download {
  version: string;
  artifact_url: string | null;
}

export function getDownloads(): Promise<{ downloads: Record<string, Download> }> {
  return apiFetch('/validator/downloads');
}
