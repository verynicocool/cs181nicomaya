# Adventure Game

The assignment this time is to implement collision detection and
response, as well as implement trigger collisions (e.g. player getting
hurt by enemies, enemies getting hurt by player attacks).

You're given starter code with a character who can move around with arrow keys and enemies that move randomly.
You need to:
1. Implement collision detection of the player against walls and the enemies against walls.
2. Compute the "attack region" when the player presses space bar, as a function of the player's facing direction and position.
3. Destroy enemies when they touch the attack region (when it's active)
4. Give the player a health meter (three hits) which decreases when they are touched by an enemy, and this meter should be drawn on the screen (you can use the HEART graphic).

Knockback and attack timers are implemented already in the code, but be sure to read the code and comments and understand how to activate the knockback feature.
