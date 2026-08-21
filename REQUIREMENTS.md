Excavation is a small game. The history is that an explorer found a precious gem in a excavation. But once it found it, the entire celling collapsed and ancient beasts (lizards very similar to dinosaurs) are trying to get him. So now you need to excavate your way to the surface and dodge the beasts. But not all rocks are mineable. Some are too hard to mine, but they look the same. So you need to try and discover which one of the rocks is mineable to find a path to the exit.

In the first version, we will have 10 levels (maps). Each level will be harder than the previous one. To make it harder, we will increase the number of unmineable rocks. Increase the speed of the beasts. In some levels add more beasts as well.

The map will have a border and two doors. One where the player start. And the other door is the exit. A beast will always start at the exit door, but we can have more beasts starting at other places. The rest of the map can be some visible walls (gray structures) added just to make the map nicer in more difficult levels. The rest of the map will have the same texture, and cane be either mineable or unmineable. The map will basically be a grid of cells, where each cell can be either a mineable or unmineable rock, or a visible wall. But the players can move seemles, not in a gridable manner. Which rocks are mineable or unmineable will be random at each execution. But each map will have a number of uminable rocks predetermined. And we guarantee that for all maps, we always will have a valid path from the start to the exit.

We should also have a map editor so that the developer can create the maps.

We can also have gold in the map. the gold will be in mineable rocks. After each level, the player can use any found gold to buy improvements or consumables.

We must have menus, so the player can interact with the game. Have a save option. Settings.

The player will receive score related to how fast it can escape each level. At the end of each level we will show its current score.

The player can buy walk speed upgrades. Or consumables. The consumables are "super pick", for 3 seconds the player can mine anything (except walls). Or "sticky smell", where for 5 seconds the beasts path finding will be disabled and they will walk randomly. All this upgrades should be configurable as .toml files so they are easy to iterate.

We must have a game toml file where we tune in game stuff (not player settings). Like upgradables (cost and effects), map order and etc. Each map also will have its own toml file where we define things like the amount of gold in the map and how many uminable rocks it will have.

There will be sounds.

The beasts will have a specific path finding algorithm. Where they will know where the position of the player is, and what the minable/umineable rocks adjacent to the beast are. As it excavate, it will reveal more and more always understading which of the rocks are mineable and which are not. It will try to go in the direction of the player, but if it cannot, it will try to go in the direction of the nearest mineable rock. We can use a A* path finding algorithm to find the shortest path to the player or the nearest mineable rock. If it can see a straitgh path to the player, it will use it.

The graphics will be in 2D, seeing from above. 

It will be written in Rust. Use the macroquad crate (https://macroquad.rs/). It should run both in the browser and on the desktop (with some small features changes like the save/load if necessary). Every dependency to the project should be added using a cargo add command, so we guarantee to add the most recent version.  

For game logic, we should write tests. But we should not need them fro visual stuff. For visual things, we should manually start the game and validate them.
