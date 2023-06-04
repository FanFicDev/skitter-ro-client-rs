create table if not exists web (
	id int8 primary key,
	created text not null,
	url text not null,
	status int2 not null,
	response blob not null
);
