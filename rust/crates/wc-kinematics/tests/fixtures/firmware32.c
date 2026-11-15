/* Holey 51.27's Kinematics::_triangularInverse and _oldTriangularInverse (cnc_ctrl_v1/Kinematics.cpp), copied with
 * every literal and libm call in float, as avr-gcc compiles it (double is float there), plus the forward solve
 * WebControl runs on it. Build with -ffp-contract=off: the AVR has no fused multiply-add. regenerate.sh runs it. */
#include <math.h>
#include <stdio.h>

typedef struct {
    float distBetweenMotors, machineWidth, machineHeight, motorOffsetY, R, sledWeight, chainElongationFactor;
    float leftChainTolerance, rightChainTolerance, rotationDiskRadius, chainSagCorrection;
    int chainOverSprocket;
} Machine;

static void oldTriangularInverse(const Machine *m, float xTarget, float yTarget, float *aChainLength,
                                 float *bChainLength) {
    float halfWidth = m->machineWidth / 2.0f, halfHeight = m->machineHeight / 2.0f;
    xTarget = (xTarget < -halfWidth) ? -halfWidth : (xTarget > halfWidth) ? halfWidth : xTarget;
    yTarget = (yTarget < -halfHeight) ? -halfHeight : (yTarget > halfHeight) ? halfHeight : yTarget;
    float _xCordOfMotor = m->distBetweenMotors / 2;
    float _yCordOfMotor = halfHeight + m->motorOffsetY;
    float R = m->R;
    float Chain1Angle, Chain2Angle, Chain1AroundSprocket, Chain2AroundSprocket;

    float Motor1Distance = sqrtf(powf((-1 * _xCordOfMotor - xTarget), 2) + powf((_yCordOfMotor - yTarget), 2));
    float Motor2Distance = sqrtf(powf((_xCordOfMotor - xTarget), 2) + powf((_yCordOfMotor - yTarget), 2));
    if (m->chainOverSprocket == 1) {
        Chain1Angle = asinf((_yCordOfMotor - yTarget) / Motor1Distance) + asinf(R / Motor1Distance);
        Chain2Angle = asinf((_yCordOfMotor - yTarget) / Motor2Distance) + asinf(R / Motor2Distance);
        Chain1AroundSprocket = R * Chain1Angle;
        Chain2AroundSprocket = R * Chain2Angle;
    } else {
        Chain1Angle = asinf((_yCordOfMotor - yTarget) / Motor1Distance) - asinf(R / Motor1Distance);
        Chain2Angle = asinf((_yCordOfMotor - yTarget) / Motor2Distance) - asinf(R / Motor2Distance);
        Chain1AroundSprocket = R * (3.14159f - Chain1Angle);
        Chain2AroundSprocket = R * (3.14159f - Chain2Angle);
    }
    float Chain1Straight = sqrtf(powf(Motor1Distance, 2) - powf(R, 2));
    float Chain2Straight = sqrtf(powf(Motor2Distance, 2) - powf(R, 2));
    Chain1Straight *= (1 + ((m->chainSagCorrection / 1000000000000.0f) * powf(cosf(Chain1Angle), 2) *
                            powf(Chain1Straight, 2) * powf((tanf(Chain2Angle) * cosf(Chain1Angle)) + sinf(Chain1Angle), 2)));
    Chain2Straight *= (1 + ((m->chainSagCorrection / 1000000000000.0f) * powf(cosf(Chain2Angle), 2) *
                            powf(Chain2Straight, 2) * powf((tanf(Chain1Angle) * cosf(Chain2Angle)) + sinf(Chain2Angle), 2)));
    float Chain1 = Chain1AroundSprocket + Chain1Straight / (1.0f + m->leftChainTolerance / 100.0f);
    float Chain2 = Chain2AroundSprocket + Chain2Straight / (1.0f + m->rightChainTolerance / 100.0f);
    *aChainLength = Chain1 - m->rotationDiskRadius;
    *bChainLength = Chain2 - m->rotationDiskRadius;
}

static void triangularInverse(const Machine *m, float xTarget, float yTarget, float *aChainLength, float *bChainLength) {
    if (m->sledWeight <= 0) {
        oldTriangularInverse(m, xTarget, yTarget, aChainLength, bChainLength);
        return;
    }
    float halfHeight = m->machineHeight / 2.0f;
    float _xCordOfMotor = m->distBetweenMotors / 2;
    float _yCordOfMotor = halfHeight + m->motorOffsetY;
    float R = m->R;
    float Chain1Angle, Chain2Angle, Chain1AroundSprocket, Chain2AroundSprocket;
    float xTangent1, yTangent1, xTangent2, yTangent2;

    float Motor1Distance = sqrtf(powf((-1.0f * _xCordOfMotor - xTarget), 2) + powf((_yCordOfMotor - yTarget), 2));
    float Motor2Distance = sqrtf(powf((_xCordOfMotor - xTarget), 2) + powf((_yCordOfMotor - yTarget), 2));

    if (m->chainOverSprocket == 1) {
        Chain1Angle = asinf((_yCordOfMotor - yTarget) / Motor1Distance) + asinf(R / Motor1Distance);
        Chain2Angle = asinf((_yCordOfMotor - yTarget) / Motor2Distance) + asinf(R / Motor2Distance);
        Chain1AroundSprocket = R * Chain1Angle;
        Chain2AroundSprocket = R * Chain2Angle;
        xTangent1 = -1.0f * _xCordOfMotor + R * sinf(Chain1Angle);
        yTangent1 = _yCordOfMotor + R * cosf(Chain1Angle);
        xTangent2 = _xCordOfMotor - R * sinf(Chain2Angle);
        yTangent2 = _yCordOfMotor + R * cosf(Chain2Angle);
    } else {
        Chain1Angle = asinf((_yCordOfMotor - yTarget) / Motor1Distance) - asinf(R / Motor1Distance);
        Chain2Angle = asinf((_yCordOfMotor - yTarget) / Motor2Distance) - asinf(R / Motor2Distance);
        Chain1AroundSprocket = R * (3.14159f - Chain1Angle);
        Chain2AroundSprocket = R * (3.14159f - Chain2Angle);
        xTangent1 = -1.0f * _xCordOfMotor - R * sinf(Chain1Angle);
        yTangent1 = _yCordOfMotor - R * cosf(Chain1Angle);
        xTangent2 = _xCordOfMotor + R * sinf(Chain2Angle);
        yTangent2 = _yCordOfMotor - R * cosf(Chain2Angle);
    }
    float sledWeight = m->sledWeight;
    float chainDensity = 0.14f * 9.8f / 1000;
    float chainElasticity = m->chainElongationFactor;

    float Chain1Straight = sqrtf(powf(Motor1Distance, 2) - powf(R, 2));
    float Chain2Straight = sqrtf(powf(Motor2Distance, 2) - powf(R, 2));
    float totalWeight = sledWeight + 0.5f * chainDensity * (Chain1Straight + Chain2Straight);
    float TensionDenominator = (xTangent1 * yTangent2 - xTangent2 * yTangent1 - xTangent1 * yTarget + xTarget * yTangent1 +
                                xTangent2 * yTarget - xTarget * yTangent2);
    float Tension1 = -(totalWeight * sqrtf(powf(xTangent1 - xTarget, 2.0f) + powf(yTangent1 - yTarget, 2.0f)) *
                       (xTangent2 - xTarget)) / TensionDenominator;
    float Tension2 = (totalWeight * sqrtf(powf(xTangent2 - xTarget, 2.0f) + powf(yTangent2 - yTarget, 2.0f)) *
                      (xTangent1 - xTarget)) / TensionDenominator;
    float HorizontalTension = Tension1 * (xTarget - xTangent1) / Chain1Straight;
    float a1 = HorizontalTension / chainDensity;
    float a2 = HorizontalTension / chainDensity;
    float Chain1 = sqrtf(powf(2 * a1 * sinhf((xTarget - xTangent1) / (2 * a1)), 2) + powf(yTangent1 - yTarget, 2));
    float Chain2 = sqrtf(powf(2 * a2 * sinhf((xTangent2 - xTarget) / (2 * a2)), 2) + powf(yTangent2 - yTarget, 2));
    Chain1 = Chain1AroundSprocket + Chain1 / (1.0f + m->leftChainTolerance / 100.0f) / (1.0f + Tension1 * chainElasticity);
    Chain2 = Chain2AroundSprocket + Chain2 / (1.0f + m->rightChainTolerance / 100.0f) / (1.0f + Tension2 * chainElasticity);
    *aChainLength = Chain1 - m->rotationDiskRadius;
    *bChainLength = Chain2 - m->rotationDiskRadius;
}

/* Kinematics::forward's step in float, with WebControl's 0.001 mm stop and 5000-step give-up at 0.1 mm. */
static int forward(const Machine *m, float left, float right, float *x, float *y) {
    for (int count = 0;; count++) {
        float guessLeft, guessRight;
        triangularInverse(m, *x, *y, &guessLeft, &guessRight);
        float a = left - guessLeft, b = right - guessRight;
        if (!isfinite(a) || !isfinite(b)) return 0;
        if (fabsf(a) < 0.001f && fabsf(b) < 0.001f) return 1;
        if (count > 5000) return fabsf(a) < 0.1f && fabsf(b) < 0.1f;
        *x = *x + 0.1f * a - 0.1f * b;
        *y = *y - 0.1f * a - 0.1f * b;
    }
}

static void machine_json(const Machine *m) {
    printf("{\"d\": %.9g, \"machineWidth\": %.9g, \"machineHeight\": %.9g, \"motorOffsetY\": %.9g, \"r\": %.9g, "
           "\"sledWeight\": %.9g, \"chainElasticity\": %.9g, \"leftChainTolerance\": %.9g, "
           "\"rightChainTolerance\": %.9g, \"rotationDiskRadius\": %.9g, \"chainSagCorrection\": %.9g, "
           "\"chainOverSprocket\": %d}",
           m->distBetweenMotors, m->machineWidth, m->machineHeight, m->motorOffsetY, m->R, m->sledWeight,
           m->chainElongationFactor, m->leftChainTolerance, m->rightChainTolerance, m->rotationDiskRadius,
           m->chainSagCorrection, m->chainOverSprocket);
}

int main(void) {
    const float R = 63.5f / (2.0f * 3.14159f);
    const Machine machines[] = {
        {2978.4f, 2438.4f, 1219.2f, 463.0f, R, 97.9f, 5.1685e-6f, 0.0f, 0.0f, 139.1f, 0.0f, 2},
        {3011.16237f, 2438.4f, 1219.2f, 433.68186f, R, 97.9f, 5.1685e-6f, -0.06495f, -0.02111f, 138.4f, 0.0f, 1},
        {2978.4f, 2438.4f, 1219.2f, 463.0f, R, 0.0f, 5.1685e-6f, 0.3f, -0.2f, 139.1f, 33.0f, 2},
    };
    const float targets[][2] = {{0, 0}, {-965.2f, 355.6f}, {965.2f, -355.6f}, {-1219.2f, -609.6f}, {12.34f, 567.8f},
                                {-1500.0f, 700.0f}};
    const float chains[][2] = {{1650.0f, 1651.0f}, {1500.25f, 2100.75f}, {2400.5f, 1350.0f}};
    const float guesses[][2] = {{0, 0}, {-10, 10}, {300, -200}};
    printf("{\n  \"machines\": [\n");
    const int count = sizeof machines / sizeof machines[0];
    for (int i = 0; i < count; i++) {
        const Machine *m = &machines[i];
        printf("    {\"machine\": ");
        machine_json(m);
        printf(",\n     \"inverse\": [");
        for (int t = 0; t < 6; t++) {
            float left, right;
            triangularInverse(m, targets[t][0], targets[t][1], &left, &right);
            printf("%s\n       {\"x\": %.9g, \"y\": %.9g, \"left\": %.9g, \"right\": %.9g}", t ? "," : "", targets[t][0],
                   targets[t][1], left, right);
        }
        printf("],\n     \"forward\": [");
        for (int c = 0; c < 3; c++) {
            float x = guesses[c][0], y = guesses[c][1];
            int found = forward(m, chains[c][0], chains[c][1], &x, &y);
            printf("%s\n       {\"left\": %.9g, \"right\": %.9g, \"guess\": [%.9g, %.9g], \"position\": ", c ? "," : "",
                   chains[c][0], chains[c][1], guesses[c][0], guesses[c][1]);
            found ? printf("[%.9g, %.9g]}", x, y) : printf("null}");
        }
        printf("]}%s\n", i == count - 1 ? "" : ",");
    }
    printf("  ]\n}\n");
    return 0;
}
