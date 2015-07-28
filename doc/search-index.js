var searchIndex = {};
searchIndex['mlv'] = {"items":[],"paths":[]};
searchIndex['melvin'] = {"items":[[0,"","melvin","Melvin is pure Rust library for configuring [LVM](https://www.sourceware.org/lvm2/).",null,null],[0,"parser","","Parsing LVM's text-based configuration format.",null,null],[4,"Entry","melvin::parser","Each value in an LvmTextMap is an Entry.",null,null],[13,"Number","","An integral numeric value",0,null],[13,"String","","A text string",0,null],[13,"List","","An ordered list of strings and numbers, possibly both",0,null],[13,"TextMap","","A nested LvmTextMap",0,null],[5,"buf_to_textmap","","Generate an `LvmTextMap` from a textual LVM configuration string.",null,null],[5,"vg_from_textmap","","Construct a `VG` from its name and an `LvmTextMap`.",null,{"inputs":[{"name":"str"},{"name":"lvmtextmap"}],"output":{"name":"result"}}],[5,"textmap_to_buf","","Generate a textual LVM configuration string from an LvmTextMap.",null,{"inputs":[{"name":"lvmtextmap"}],"output":{"name":"vec"}}],[6,"LvmTextMap","","A Map that represents LVM metadata.",null,null],[8,"TextMapOps","","Operations that can be used to extract values from an `LvmTextMap`.",null,null],[10,"i64_from_textmap","","Get an i64 value from a LvmTextMap.",1,{"inputs":[{"name":"textmapops"},{"name":"str"}],"output":{"name":"option"}}],[10,"string_from_textmap","","Get a reference to a string in an LvmTextMap.",1,{"inputs":[{"name":"textmapops"},{"name":"str"}],"output":{"name":"option"}}],[10,"list_from_textmap","","Get a reference to a List within an LvmTextMap.",1,{"inputs":[{"name":"textmapops"},{"name":"str"}],"output":{"name":"option"}}],[10,"textmap_from_textmap","","Get a reference to a nested LvmTextMap within an LvmTextMap.",1,{"inputs":[{"name":"textmapops"},{"name":"str"}],"output":{"name":"option"}}],[11,"clone","","",0,{"inputs":[{"name":"entry"}],"output":{"name":"entry"}}],[11,"eq","","",0,{"inputs":[{"name":"entry"},{"name":"entry"}],"output":{"name":"bool"}}],[11,"ne","","",0,{"inputs":[{"name":"entry"},{"name":"entry"}],"output":{"name":"bool"}}],[11,"fmt","","",0,{"inputs":[{"name":"entry"},{"name":"formatter"}],"output":{"name":"result"}}],[11,"i64_from_textmap","","",2,{"inputs":[{"name":"lvmtextmap"},{"name":"str"}],"output":{"name":"option"}}],[11,"string_from_textmap","","",2,{"inputs":[{"name":"lvmtextmap"},{"name":"str"}],"output":{"name":"option"}}],[11,"textmap_from_textmap","","",2,{"inputs":[{"name":"lvmtextmap"},{"name":"str"}],"output":{"name":"option"}}],[11,"list_from_textmap","","",2,{"inputs":[{"name":"lvmtextmap"},{"name":"str"}],"output":{"name":"option"}}],[0,"lvmetad","melvin","Communicating with `lvmetad`.",null,null],[5,"request","melvin::lvmetad","Make a request to the running lvmetad daemon.",null,null],[5,"vgs_from_lvmetad","","Query `lvmetad` for a list of Volume Groups on the system.",null,{"inputs":[],"output":{"name":"result"}}],[5,"vg_update_lvmetad","","Tell `lvmetad` about the current state of a Volume Group.",null,{"inputs":[{"name":"lvmtextmap"}],"output":{"name":"result"}}],[0,"pvlabel","melvin","Reading and writing LVM on-disk labels and metadata.",null,null],[3,"MDA","melvin::pvlabel","A handle to an LVM on-disk metadata area (MDA)",null,null],[5,"scan_for_pvs","","Scan a list of directories for block devices containing LVM PV labels.",null,null],[11,"new","","Construct an MDA given a path to a block device containing an LVM Physical Volume (PV)",3,{"inputs":[{"name":"mda"},{"name":"path"}],"output":{"name":"result"}}],[11,"read_metadata","","Read the metadata contained in the metadata area.",3,{"inputs":[{"name":"mda"}],"output":{"name":"result"}}],[11,"write_metadata","","Write a new version of the metadata to the metadata area.",3,{"inputs":[{"name":"mda"},{"name":"lvmtextmap"}],"output":{"name":"result"}}],[0,"dm","melvin","Communicating with the running kernel using devicemapper.",null,null],[3,"DM","melvin::dm","Context needed for communicating with devicemapper.",null,null],[11,"new","","Create a new context for communicating about a given VG with DM.",4,{"inputs":[{"name":"dm"},{"name":"vg"}],"output":{"name":"result"}}],[11,"get_version","","Devicemapper version information: Major, Minor, and patchlevel versions.",4,{"inputs":[{"name":"dm"}],"output":{"name":"result"}}],[11,"list_devices","","Returns a list of tuples containing DM device names and their major/minor\ndevice numbers.",4,{"inputs":[{"name":"dm"}],"output":{"name":"result"}}],[11,"activate_device","","Activate a Logical Volume.",4,{"inputs":[{"name":"dm"},{"name":"lv"}],"output":{"name":"result"}}],[11,"remove_device","","Remove a Logical Volume.",4,{"inputs":[{"name":"dm"},{"name":"lv"}],"output":{"name":"result"}}],[0,"lv","melvin","Logical Volumes",null,null],[3,"LV","melvin::lv","A Logical Volume.",null,null],[12,"name","","The name.",5,null],[12,"id","","The UUID.",5,null],[12,"status","","The status.",5,null],[12,"flags","","Flags.",5,null],[12,"creation_host","","Created by this host.",5,null],[12,"creation_time","","Created at this Unix time.",5,null],[12,"segments","","A list of the segments comprising the LV.",5,null],[12,"device","","The major/minor number of the LV.",5,null],[3,"Segment","","A Logical Volume Segment.",null,null],[12,"name","","A mostly-useless name.",6,null],[12,"start_extent","","The first extent within the LV this segment comprises.",6,null],[12,"extent_count","","How many extents this segment comprises",6,null],[12,"ty","","The segment type.",6,null],[12,"stripes","","If >1, Segment is striped across multiple PVs.",6,null],[11,"clone","","",5,{"inputs":[{"name":"lv"}],"output":{"name":"lv"}}],[11,"eq","","",5,{"inputs":[{"name":"lv"},{"name":"lv"}],"output":{"name":"bool"}}],[11,"ne","","",5,{"inputs":[{"name":"lv"},{"name":"lv"}],"output":{"name":"bool"}}],[11,"fmt","","",5,{"inputs":[{"name":"lv"},{"name":"formatter"}],"output":{"name":"result"}}],[11,"used_extents","","The total number of extents used by this logical volume.",5,{"inputs":[{"name":"lv"}],"output":{"name":"u64"}}],[11,"from","","",2,{"inputs":[{"name":"lvmtextmap"},{"name":"lv"}],"output":{"name":"lvmtextmap"}}],[11,"clone","","",6,{"inputs":[{"name":"segment"}],"output":{"name":"segment"}}],[11,"eq","","",6,{"inputs":[{"name":"segment"},{"name":"segment"}],"output":{"name":"bool"}}],[11,"ne","","",6,{"inputs":[{"name":"segment"},{"name":"segment"}],"output":{"name":"bool"}}],[11,"fmt","","",6,{"inputs":[{"name":"segment"},{"name":"formatter"}],"output":{"name":"result"}}],[11,"from","","",2,{"inputs":[{"name":"lvmtextmap"},{"name":"segment"}],"output":{"name":"lvmtextmap"}}],[0,"vg","melvin","Volume Groups",null,null],[3,"VG","melvin::vg","A Volume Group.",null,null],[12,"name","","Name.",7,null],[12,"id","","Uuid.",7,null],[12,"seqno","","The generation of metadata this VG represents.",7,null],[12,"format","","Always \"LVM2\".",7,null],[12,"status","","Status.",7,null],[12,"flags","","Flags.",7,null],[12,"extent_size","","Size of each extent, in 512-byte sectors.",7,null],[12,"max_lv","","Maximum number of LVs, 0 means no limit.",7,null],[12,"max_pv","","Maximum number of PVs, 0 means no limit.",7,null],[12,"metadata_copies","","How many metadata copies (?)",7,null],[12,"pvs","","Physical Volumes within this volume group.",7,null],[12,"lvs","","Logical Volumes within this volume group.",7,null],[11,"clone","","",7,{"inputs":[{"name":"vg"}],"output":{"name":"vg"}}],[11,"eq","","",7,{"inputs":[{"name":"vg"},{"name":"vg"}],"output":{"name":"bool"}}],[11,"ne","","",7,{"inputs":[{"name":"vg"},{"name":"vg"}],"output":{"name":"bool"}}],[11,"fmt","","",7,{"inputs":[{"name":"vg"},{"name":"formatter"}],"output":{"name":"result"}}],[11,"extents_in_use","","The total number of extents in use in the volume group.",7,{"inputs":[{"name":"vg"}],"output":{"name":"u64"}}],[11,"extents_free","","The total number of free extents in the volume group.",7,{"inputs":[{"name":"vg"}],"output":{"name":"u64"}}],[11,"extents","","The total number of extents in the volume group.",7,{"inputs":[{"name":"vg"}],"output":{"name":"u64"}}],[11,"new_linear_lv","","Create a new linear logical volume in the volume group.",7,{"inputs":[{"name":"vg"},{"name":"str"},{"name":"u64"}],"output":{"name":"result"}}],[11,"lv_remove","","Destroy a logical volume.",7,{"inputs":[{"name":"vg"},{"name":"str"}],"output":{"name":"result"}}],[11,"from","","",2,{"inputs":[{"name":"lvmtextmap"},{"name":"vg"}],"output":{"name":"self"}}],[0,"pv","melvin","Physical Volumes",null,null],[3,"Device","melvin::pv","A struct containing the device's major and minor numbers",null,null],[12,"major","","Device major number",8,null],[12,"minor","","Device minor number",8,null],[3,"PV","","A Physical Volume.",null,null],[12,"name","","The mostly-useless name",9,null],[12,"id","","Its UUID",9,null],[12,"device","","Device number for the block device the PV is on",9,null],[12,"status","","Status",9,null],[12,"flags","","Flags",9,null],[12,"dev_size","","The device's size, in bytes",9,null],[12,"pe_start","","The offset in sectors of where the first extent starts",9,null],[12,"pe_count","","The number of extents in the PV",9,null],[4,"LvmDeviceError","","Errors that can occur when converting from a String into a Device",null,null],[13,"IoError","","IO Error",10,null],[11,"clone","","",8,{"inputs":[{"name":"device"}],"output":{"name":"device"}}],[11,"eq","","",8,{"inputs":[{"name":"device"},{"name":"device"}],"output":{"name":"bool"}}],[11,"ne","","",8,{"inputs":[{"name":"device"},{"name":"device"}],"output":{"name":"bool"}}],[11,"fmt","","",8,{"inputs":[{"name":"device"},{"name":"formatter"}],"output":{"name":"result"}}],[11,"path","","Returns the path in `/dev` that corresponds with the device number",8,{"inputs":[{"name":"device"}],"output":{"name":"option"}}],[11,"from_str","","",8,{"inputs":[{"name":"device"},{"name":"str"}],"output":{"name":"result"}}],[11,"from","","",8,{"inputs":[{"name":"device"},{"name":"i64"}],"output":{"name":"device"}}],[11,"clone","","",9,{"inputs":[{"name":"pv"}],"output":{"name":"pv"}}],[11,"eq","","",9,{"inputs":[{"name":"pv"},{"name":"pv"}],"output":{"name":"bool"}}],[11,"ne","","",9,{"inputs":[{"name":"pv"},{"name":"pv"}],"output":{"name":"bool"}}],[11,"fmt","","",9,{"inputs":[{"name":"pv"},{"name":"formatter"}],"output":{"name":"result"}}],[11,"from","","",2,{"inputs":[{"name":"lvmtextmap"},{"name":"pv"}],"output":{"name":"lvmtextmap"}}]],"paths":[[4,"Entry"],[8,"TextMapOps"],[6,"LvmTextMap"],[3,"MDA"],[3,"DM"],[3,"LV"],[3,"Segment"],[3,"VG"],[3,"Device"],[3,"PV"],[4,"LvmDeviceError"]]};
initSearch(searchIndex);
