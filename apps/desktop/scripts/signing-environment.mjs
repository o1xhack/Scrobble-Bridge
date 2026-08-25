export function createSigningEnvironment(environment, signingIdentity) {
  const buildEnvironment = { ...environment };
  if (signingIdentity) {
    buildEnvironment.APPLE_SIGNING_IDENTITY = signingIdentity;
  } else {
    delete buildEnvironment.APPLE_SIGNING_IDENTITY;
  }
  return buildEnvironment;
}
