#version 460 core
precision mediump float;

in vec2 v_texCoordinate;

out vec4 f_fragColor;

const float radius = 400.0;
const vec2 center = vec2(400, 400);

float distanceFromCenter() {
    float x_component = pow(gl_FragCoord.x - center.x, 2);
    float y_component = pow(gl_FragCoord.y - center.y, 2);

    return sqrt(x_component + y_component);
}

const vec4 faceColor = vec4(0.2, 0.2, 0.3, 1.0);
const vec4 transparentBlack = vec4(0.0, 0.0, 0.0, 0.0);
const vec4 aaColor = vec4(0.1, 0.1, 0.15, 0.5);

uniform sampler2D faceTexture;

void main() {
    //    float distanceFromCenter = distanceFromCenter();
    //
    //    if (distanceFromCenter <= radius) {
    //        f_fragColor = faceColor;
    //    } else if (distanceFromCenter <= (radius + 1)) {
    //        f_fragColor = aaColor;
    //    } else {
    //        f_fragColor = transparentBlack;
    //    }

    f_fragColor = texture(faceTexture, v_texCoordinate);
}