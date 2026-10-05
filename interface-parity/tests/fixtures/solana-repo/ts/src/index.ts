export interface Deployment {
  chainId: string;
  address: string;
}

export function adapterAddress(environment: "staging" | "production"): string | undefined {
  return environment === "staging" ? "So1" : undefined;
}

export const VERSION = "1.0.0";
